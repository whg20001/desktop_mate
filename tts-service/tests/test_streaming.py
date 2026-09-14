import sys
import unittest
from pathlib import Path
from types import SimpleNamespace
sys.path.insert(0, str(Path(__file__).parents[1]))
from streaming import generate_chunks
try:
    import torch
except ImportError:
    torch = None

@unittest.skipIf(torch is None, "Requires the qwen3-tts environment")
class StreamingTests(unittest.TestCase):
    def fake(self):
        class Talker:
            hook = None
            def register_forward_hook(self, hook):
                self.hook = hook
                return SimpleNamespace(remove=lambda: setattr(self, "hook", None))
        talker = Talker()
        state = SimpleNamespace(finished=False, contexts=[])
        def decode(items):
            frames = items[0]["audio_codes"]
            state.contexts.append(len(frames))
            return [frames[:, 0].repeat_interleave(4).numpy()], 24000
        def generate(**kwargs):
            talker.hook(None, None, SimpleNamespace(hidden_states=(None, None)))
            for i in range(35):
                talker.hook(None, None, SimpleNamespace(hidden_states=(None, torch.tensor([[i, i]]))))
            talker.hook(None, None, SimpleNamespace(hidden_states=(None, torch.tensor([[999, 0]]))))
            state.finished = True
        model = SimpleNamespace(_tokenize_texts=lambda x:x, _build_assistant_text=lambda x:x,
            _merge_generate_kwargs=lambda **kw:kw,
            model=SimpleNamespace(talker=talker, generate=generate,
                config=SimpleNamespace(talker_config=SimpleNamespace(codec_eos_token_id=999)),
                speech_tokenizer=SimpleNamespace(decode=decode, model=SimpleNamespace(get_decode_upsample_rate=lambda:4))))
        return model, state

    def test_early_chunks_preserve_every_sample_and_flush_tail(self):
        model, state = self.fake()
        output, indices, early = [], [], []
        def emit(wav, rate, index, timing):
            output.extend(wav.tolist());indices.append(index);early.append(not state.finished)
            self.assertGreaterEqual(timing["modelMs"], 0)
        result=generate_chunks(model, "test", "serena", "chinese", lambda:None, emit)
        self.assertEqual(output, [i for i in range(35) for _ in range(4)])
        self.assertEqual(indices, [0,1,2,3])
        self.assertEqual(early, [True,True,True,False])
        self.assertLessEqual(max(state.contexts), 37)
        self.assertEqual(result["chunks"],4)
        self.assertIsNone(model.model.talker.hook)

    def test_failure_during_output_removes_hook_and_stops_generation(self):
        model, state = self.fake()
        def fail(*args): raise RuntimeError("client cancelled")
        with self.assertRaisesRegex(RuntimeError,"client cancelled"):
            generate_chunks(model,"test","serena","chinese",lambda:None,fail)
        self.assertFalse(state.finished)
        self.assertIsNone(model.model.talker.hook)

if __name__ == '__main__': unittest.main()
