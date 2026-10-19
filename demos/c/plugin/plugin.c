#include "morph_reload.h"

#include <stdio.h>

#define MORPH_DEMO_VERSION 1

static char output[128];
static const char plugin_name[] = "morph-c-demo";

static const char *render(uint64_t host_state) {
    (void)snprintf(
        output,
        sizeof(output),
        "version=%d count=%llu",
        MORPH_DEMO_VERSION,
        (unsigned long long)host_state
    );
    return output;
}

static const MorphPluginV1 plugin = {
    MORPH_PLUGIN_ABI_VERSION,
    plugin_name,
    render,
};

MORPH_PLUGIN_EXPORT const MorphPluginV1 *morph_plugin_v1(void) {
    return &plugin;
}
