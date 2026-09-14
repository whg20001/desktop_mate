"""Run with: conda run -n qwen3-tts python tts-service/check_environment.py"""
import importlib.metadata
import json
import sys
from pathlib import Path
from service import resolve_model

def main():
    report = {"python": sys.executable, "version": sys.version.split()[0]}
    for package in ["qwen-tts", "torch", "torchaudio", "transformers", "soundfile", "accelerate"]:
        try: report[package] = importlib.metadata.version(package)
        except importlib.metadata.PackageNotFoundError: report[package] = "MISSING"
    import torch
    report["cuda"] = torch.version.cuda
    report["cudaAvailable"] = torch.cuda.is_available()
    if report["cudaAvailable"]:
        report["device"] = torch.cuda.get_device_name(0)
        report["cudaTensorCheck"] = float((torch.ones((8, 8), device="cuda") @ torch.ones((8, 8), device="cuda")).sum())
    catalog = json.loads((Path(__file__).parent / "models.json").read_text(encoding="utf-8"))
    failures = []
    for entry in catalog["models"]:
        try: report[entry["id"]] = str(resolve_model(entry["modelPath"], entry.get("revision")))
        except Exception as error: failures.append(str(error))
    report["errors"] = failures
    print(json.dumps(report, indent=2, ensure_ascii=True))
    return 0 if report["cudaAvailable"] and not failures and "MISSING" not in report.values() else 1

if __name__ == "__main__": raise SystemExit(main())
