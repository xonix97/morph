#ifndef MORPH_RELOAD_H
#define MORPH_RELOAD_H

#include <stdint.h>

#define MORPH_PLUGIN_ABI_VERSION 1u

#if defined(_WIN32)
#define MORPH_PLUGIN_EXPORT __declspec(dllexport)
#else
#define MORPH_PLUGIN_EXPORT
#endif

/*
 * Stable boundary between a long-lived host and a reloadable C module.
 *
 * The host owns all durable state. The module returns a pointer to memory it
 * owns and must keep valid until the next call or until the module generation
 * is no longer used. Modules must not retain host pointers or spawn callbacks
 * into themselves after a generation switch.
 */
typedef struct MorphPluginV1 {
    uint32_t abi_version;
    const char *name;
    const char *(*render)(uint64_t host_state);
} MorphPluginV1;

#ifdef __cplusplus
extern "C" {
#endif

MORPH_PLUGIN_EXPORT const MorphPluginV1 *morph_plugin_v1(void);

#ifdef __cplusplus
}
#endif

#endif
