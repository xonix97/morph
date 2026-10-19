MORPH_RELOAD_VERSION = 1


def render(state):
    return f"python-v{MORPH_RELOAD_VERSION} ticks={state['ticks']}"
