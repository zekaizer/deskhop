/* DeskHop DMA buffer + reboot + firmware request. */
#include "main.h"

extern uint32_t rust_get_ptr_delta(uint32_t, uint32_t, uint32_t);

uint32_t get_ptr_delta(uint32_t cp, device_t *s) { return rust_get_ptr_delta(cp, s->dma_ptr, DMA_RX_BUFFER_SIZE); }

bool is_start_of_packet(device_t *s) {
    return uart_rxbuf[s->dma_ptr] == START1 && uart_rxbuf[NEXT_RING_IDX(s->dma_ptr)] == START2;
}

void fetch_packet(device_t *state) {
    uint8_t *dst = (uint8_t *)&state->in_packet;
    for (int i = 0; i < RAW_PACKET_LENGTH; i++) {
        if (i >= START_LENGTH) dst[i - START_LENGTH] = uart_rxbuf[state->dma_ptr];
        state->dma_ptr = NEXT_RING_IDX(state->dma_ptr);
    }
}

void request_byte(device_t *state, uint32_t address) {
    uart_packet_t p = { .data32[0] = address, .type = REQUEST_BYTE_MSG };
    state->fw.byte_done = false;
    queue_try_add(&global_state.uart_tx_queue, &p);
}

void reboot(void) { *((volatile uint32_t*)(PPB_BASE + 0x0ED0C)) = 0x5FA0004; }
