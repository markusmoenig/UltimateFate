#ifndef ULTIMATE_FATE_H
#define ULTIMATE_FATE_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct UltimateFateAppleClient UltimateFateAppleClient;

/*
 * Device-neutral controls. Native keyboard, GCController, touch, and
 * GCKeyboard adapters all map into this same vocabulary.
 */
typedef enum UltimateFateInput {
    ULTIMATE_FATE_INPUT_NORTH = 0,
    ULTIMATE_FATE_INPUT_EAST = 1,
    ULTIMATE_FATE_INPUT_SOUTH = 2,
    ULTIMATE_FATE_INPUT_WEST = 3,
    ULTIMATE_FATE_INPUT_PRIMARY = 4,
    ULTIMATE_FATE_INPUT_BACK = 5,
    ULTIMATE_FATE_INPUT_INSPECT = 6,
    ULTIMATE_FATE_INPUT_JOURNAL = 7,
    ULTIMATE_FATE_INPUT_MENU = 8
} UltimateFateInput;

/*
 * layer must point to a CAMetalLayer and must remain alive until the client is
 * destroyed. width and height are physical drawable pixels; scale is the native
 * display scale used to keep game UI and cells a stable point size.
 */
UltimateFateAppleClient *ultimate_fate_client_create(
    void *layer,
    uint32_t width,
    uint32_t height,
    float scale,
    uint64_t campaign_seed
);

void ultimate_fate_client_destroy(UltimateFateAppleClient *client);
void ultimate_fate_client_resize(
    UltimateFateAppleClient *client,
    uint32_t width,
    uint32_t height,
    float scale
);

/*
 * Feed elapsed wall-clock seconds from the native display loop. The Rust client
 * advances fixed simulation ticks and retains any fractional remainder.
 */
void ultimate_fate_client_update(
    UltimateFateAppleClient *client,
    double elapsed_seconds
);

/*
 * Digital input for keyboards, D-pads, controller buttons, touch buttons, and
 * the Siri Remote. Send pressed=1 on press and pressed=0 on release. Held
 * movement repetition is owned by the shared Rust input controller.
 */
void ultimate_fate_client_set_input(
    UltimateFateAppleClient *client,
    uint32_t input,
    uint32_t pressed
);

/*
 * Analog or virtual joystick movement in the range -1...1. The shared input
 * controller applies a dead zone and selects the dominant cardinal direction.
 * Pass (0, 0) when the stick or touch is released.
 */
void ultimate_fate_client_set_movement(
    UltimateFateAppleClient *client,
    float x,
    float y
);

/*
 * Commands: 0 north, 1 east, 2 south, 3 west, 4 wait, 5 toggle pause.
 * Legacy one-shot API. New native hosts should use set_input/set_movement so
 * held controls and all four gameplay buttons share the same behavior.
 */
void ultimate_fate_client_command(
    UltimateFateAppleClient *client,
    uint32_t command
);

/*
 * Returns 0 on success, 1 when the surface was reconfigured, 2 on timeout,
 * 3 on out-of-memory, and 4 for another surface error.
 */
uint32_t ultimate_fate_client_render(UltimateFateAppleClient *client);

#ifdef __cplusplus
}
#endif

#endif
