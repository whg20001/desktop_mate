"""Incremental codec-frame decoding for the pinned qwen-tts 0.1.1 API.

The forward hook observes complete code groups, not just the first codebook.
Only the last 25 context frames are decoded again; their samples are discarded.
No installed SDK files are patched. Text is still supplied as a complete request.
"""
import time


def generate_chunks(model, text, speaker, language, check, emit):
    import torch
    started = time.perf_counter()
    # These private helpers isolate the version-specific prompt adapter here.
    inputs = model._tokenize_texts([model._build_assistant_text(text)])
    tokenizer = model.model.speech_tokenizer
    stride = int(tokenizer.model.get_decode_upsample_rate())
    frames, emitted, chunks = [], 0, 0
    decode_ms = 0.0
    output_ms = 0.0
    first_chunk_ms = None

    def flush():
        nonlocal emitted, chunks, decode_ms, output_ms, first_chunk_ms
        check()
        count = len(frames)
        if count == emitted:
            return
        left = max(0, emitted - 25)
        tick = time.perf_counter()
        codes = torch.stack(frames[left:count], dim=0)
        wavs, rate = tokenizer.decode([{"audio_codes": codes}])
        wav = wavs[0][(emitted - left) * stride:]
        decode_ms += (time.perf_counter() - tick) * 1000
        check()
        if len(wav) != (count - emitted) * stride:
            raise RuntimeError("Unexpected codec chunk length")
        elapsed = (time.perf_counter() - started) * 1000
        if first_chunk_ms is None:
            first_chunk_ms = elapsed
        output_start = time.perf_counter()
        emit(wav, rate, chunks, {"modelMs": max(0, elapsed - decode_ms - output_ms),
             "codecDecodeMs": decode_ms, "firstChunkMs": first_chunk_ms})
        output_ms += (time.perf_counter() - output_start) * 1000
        emitted, chunks = count, chunks + 1

    def observe(_module, _args, output):
        check()
        codes = output.hidden_states[-1]
        if codes is None:
            return
        frame = codes[0].detach()
        if int(frame[0]) == model.model.config.talker_config.codec_eos_token_id:
            return
        frames.append(frame)
        if len(frames) >= 1440:
            raise ValueError("generated audio exceeds 120 seconds")
        # First block ~0.5 s; subsequent blocks ~1 s at 12 Hz.
        if len(frames) - emitted >= (6 if chunks == 0 else 12):
            flush()

    hook = model.model.talker.register_forward_hook(observe)
    try:
        with torch.inference_mode():
            model.model.generate(input_ids=inputs, instruct_ids=[None],
                languages=[language], speakers=[speaker], non_streaming_mode=True,
                **model._merge_generate_kwargs(max_new_tokens=1440))
            check()
            flush()
    finally:
        hook.remove()
    if not chunks:
        raise ValueError("model produced no audio")
    elapsed = (time.perf_counter() - started) * 1000
    return {"modelMs": max(0, elapsed - decode_ms - output_ms), "codecDecodeMs": decode_ms,
            "firstChunkMs": first_chunk_ms, "chunks": chunks}
