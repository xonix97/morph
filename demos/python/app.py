import os
import time

from morph_runtime import Runtime


runtime = Runtime(["app_logic"])
print(f"PYTHON_HOST_START pid={os.getpid()}", flush=True)
try:
    while runtime.running:
        runtime.state["ticks"] += 1
        value = runtime.call("app_logic", "render", runtime.state)
        print(f"PYTHON_TICK pid={os.getpid()} {value}", flush=True)
        time.sleep(0.2)
finally:
    runtime.close()
    print(f"PYTHON_HOST_STOP pid={os.getpid()} state={runtime.state['ticks']}", flush=True)
