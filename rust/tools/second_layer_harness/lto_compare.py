"""Clean release builds of the HUD under different LTO / codegen-unit settings, timed sequentially."""
import os, subprocess, sys, time
variants = {
    "fat":  {},
    "thin": {"CARGO_PROFILE_RELEASE_LTO": "thin", "CARGO_PROFILE_RELEASE_CODEGEN_UNITS": "16"},
    "off":  {"CARGO_PROFILE_RELEASE_LTO": "off",  "CARGO_PROFILE_RELEASE_CODEGEN_UNITS": "16"},
}
for name, extra in variants.items():
    env = dict(os.environ, CARGO_TARGET_DIR=f"target_b_{name}", **extra)
    t = time.time()
    r = subprocess.run(["cargo", "build", "--release", "--timings"], env=env, capture_output=True, text=True)
    wall = time.time() - t
    exe = f"target_b_{name}/release/ClaudeHUD.exe"
    print(f"{name}: rc={r.returncode} wall={wall:.0f}s exe={os.path.getsize(exe) if os.path.exists(exe) else None}", flush=True)
