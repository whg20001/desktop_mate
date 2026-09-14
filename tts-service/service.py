"""Owned stdio TTS service. JSON headers; successful synthesis adds WAV bytes.

stdout is protocol-only. stdin EOF terminates the owned process, including loading.
Models are resolved offline: installation/download is an explicit setup operation.
"""
import argparse
import contextlib
import io
import json
import os
import queue
import sys
import threading
import time
from streaming import generate_chunks
from pathlib import Path

MAX_HEADER = 65536
MAX_WAV = 64 * 1024 * 1024
MAX_TEXT = 8000
LANGUAGES = {"zh": "chinese", "en": "english", "ja": "japanese", "ko": "korean",
             "de": "german", "fr": "french", "ru": "russian", "pt": "portuguese",
             "es": "spanish", "it": "italian", "auto": "auto"}

class Interrupted(Exception):
    pass


def language_name(value):
    if not isinstance(value, str):
        raise ValueError("language must be a string")
    name = LANGUAGES.get(value.lower().split("-")[0], value.lower())
    if name not in LANGUAGES.values():
        raise ValueError("unsupported language")
    return name


def validate_request(request, speakers):
    text = request.get("text")
    if not isinstance(text, str) or not text.strip() or len(text) > MAX_TEXT or "\0" in text:
        raise ValueError("text must contain 1-8000 characters")
    speaker = request.get("speaker", "serena")
    if speaker not in speakers:
        raise ValueError("unsupported speaker")
    timeout = request.get("timeoutSeconds", 180)
    if type(timeout) not in (int, float) or not 2 <= timeout <= 180:
        raise ValueError("timeout must be between 2 and 180 seconds")
    return text, speaker, language_name(request.get("language") or "auto"), timeout


def resolve_model(model_path, revision=None):
    path = Path(model_path)
    if not path.is_dir():
        from huggingface_hub import snapshot_download
        path = Path(snapshot_download(model_path, revision=revision, local_files_only=True))
    for file in ["config.json", "model.safetensors", "tokenizer_config.json",
                 "generation_config.json", "speech_tokenizer/config.json", "speech_tokenizer/model.safetensors"]:
        if not (path / file).is_file():
            raise RuntimeError("Incomplete model cache: missing " + file)
    config = json.loads((path / "config.json").read_text(encoding="utf-8"))
    if config.get("tts_model_type") != "custom_voice":
        raise ValueError("Only Qwen3 CustomVoice models are supported by this service")
    return path


def serve(model_path, revision=None):
    # Keep a private reference before redirecting SDK progress/prints to stderr.
    output = sys.stdout.buffer
    def send(message, wav=b""):
        output.write(json.dumps(message, ensure_ascii=True).encode() + b"\n")
        if wav:
            output.write(wav)
        output.flush()

    pending = queue.Queue(maxsize=1)
    cancelled = threading.Event()
    active_id = None
    control_lock = threading.Lock()
    def read_commands():
        nonlocal active_id
        while True:
            line = sys.stdin.buffer.readline(MAX_HEADER + 1)
            if not line:
                os._exit(0)  # Parent owns this process; release CUDA on parent death.
            if len(line) > MAX_HEADER:
                os._exit(2)
            try:
                request = json.loads(line)
                if request.get("type") == "shutdown":
                    os._exit(0)
                with control_lock:
                    if request.get("type") == "cancel":
                        if request.get("id") == active_id:
                            cancelled.set()
                    elif request.get("type") == "synthesize" and active_id is None:
                        active_id = request["id"]
                        request["receivedAt"] = time.perf_counter()
                        cancelled.clear()
                        pending.put_nowait(request)
                    else:
                        os._exit(2)
            except (ValueError, TypeError, KeyError, queue.Full):
                os._exit(2)
    with contextlib.redirect_stdout(sys.stderr):
        try:
            import torch
            # Small autoregressive GPU steps suffer from excessive CPU thread fan-out.
            torch.set_num_threads(4)
            if not torch.cuda.is_available():
                raise RuntimeError("CUDA unavailable: install CUDA-enabled PyTorch in qwen3-tts")
            import soundfile as sf
            from qwen_tts import Qwen3TTSModel
            from importlib.metadata import version
            if version("qwen-tts") != "0.1.1":
                raise RuntimeError("Streaming adapter requires qwen-tts 0.1.1; validate adapter before upgrading")
            # Import native libraries before a thread blocks on piped stdin:
            # NumPy's Windows loader can deadlock against the CRT input lock.
            threading.Thread(target=read_commands, daemon=True).start()
            path = resolve_model(model_path, revision)
            model = Qwen3TTSModel.from_pretrained(str(path), device_map="cuda:0",
                        dtype=torch.bfloat16, attn_implementation="sdpa", local_files_only=True)
            speakers = list(model.get_supported_speakers())
            send({"type": "ready", "device": torch.cuda.get_device_name(0),
                  "dtype": "bfloat16", "speakers": speakers,
                  "languages": list(model.get_supported_languages())})
        except Exception as error:
            send({"type": "failed", "message": str(error)[:1000]})
            return 1
        while True:
            request = pending.get()
            request_id = request["id"]
            try:
                text, speaker, language, timeout = validate_request(request, speakers)
                deadline = time.perf_counter() + timeout
                def check(*_):
                    if cancelled.is_set():
                        raise Interrupted("cancelled")
                    if time.perf_counter() >= deadline:
                        raise Interrupted("synthesis_timeout")
                queue_ms = (time.perf_counter() - request["receivedAt"]) * 1000
                if request.get("stream", False):
                    total_bytes = 0
                    def emit_chunk(wav, rate, index, timings):
                        nonlocal total_bytes
                        check()
                        buffer = io.BytesIO()
                        sf.write(buffer, wav, rate, format="WAV", subtype="PCM_16")
                        data = buffer.getvalue()
                        total_bytes += len(data)
                        if total_bytes > MAX_WAV:
                            raise ValueError("generated audio exceeds memory limit")
                        send({"type": "chunk", "id": request_id, "index": index,
                              "length": len(data), "timings": timings}, data)
                    timings = generate_chunks(model, text, speaker, language, check, emit_chunk)
                    timings["serviceQueueMs"] = queue_ms
                    result, data = {"type": "done", "id": request_id, "timings": timings}, b""
                else:
                    # Complete-WAV path retained for offline smoke/comparison.
                    hook = model.model.talker.register_forward_pre_hook(lambda *_: check())
                    try:
                        check()
                        wavs, rate = model.generate_custom_voice(text=text, language=language,
                                speaker=speaker, max_new_tokens=1440)
                        check()
                    finally:
                        hook.remove()
                    if len(wavs[0]) / rate >= 120:
                        raise ValueError("generated audio exceeds 120 seconds")
                    buffer = io.BytesIO()
                    sf.write(buffer, wavs[0], rate, format="WAV", subtype="PCM_16")
                    data = buffer.getvalue()
                    if len(data) > MAX_WAV:
                        raise ValueError("generated audio exceeds memory limit")
                    result = {"type": "audio", "id": request_id, "length": len(data)}
            except Interrupted as error:
                result, data = {"type": "error", "id": request_id, "code": str(error)}, b""
            except Exception:
                # Do not write user text or model exception payloads to logs.
                result, data = {"type": "error", "id": request_id, "code": "synthesis_failed"}, b""
            with control_lock:
                if cancelled.is_set():
                    result, data = {"type": "error", "id": request_id, "code": "cancelled"}, b""
                active_id = None
                send(result, data)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", required=True)
    parser.add_argument("--revision")
    args = parser.parse_args()
    for key in ["HF_HUB_OFFLINE", "TRANSFORMERS_OFFLINE", "HF_HUB_DISABLE_TELEMETRY"]:
        os.environ[key] = "1"
    return serve(args.model, args.revision)

if __name__ == "__main__":
    raise SystemExit(main())
