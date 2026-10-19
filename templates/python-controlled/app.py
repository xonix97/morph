"""Minimal long-lived Python host for Morph's controlled backend."""

from __future__ import annotations

import os
import time

from morph_runtime import Runtime


MODULE = "app_logic"


def main() -> None:
    runtime = Runtime([MODULE])
    runtime.state.update(
        {
            "ticks": 0,
            "seed": 7,
        }
    )
    print(
        f"PYTHON_TEMPLATE_START pid={os.getpid()} seed={runtime.state['seed']}",
        flush=True,
    )
    try:
        while runtime.running:
            runtime.state["ticks"] += 1
            result = runtime.call(MODULE, "render", runtime.state)
            print(
                f"PYTHON_TEMPLATE_TICK pid={os.getpid()} "
                f"version={result['version']} ticks={runtime.state['ticks']} "
                f"value={result['value']} label={result['label']}",
                flush=True,
            )
            time.sleep(0.2)
    finally:
        runtime.close()
        print(
            f"PYTHON_TEMPLATE_STOP pid={os.getpid()} "
            f"ticks={runtime.state['ticks']}",
            flush=True,
        )


if __name__ == "__main__":
    main()
