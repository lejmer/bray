#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "runtime-cleanup-abi.h"

static _Thread_local uintptr_t cleanup_shield_depth;
static _Atomic uintptr_t cleanup_shield_entries;
static _Atomic uintptr_t cleanup_shield_exits;

// This synchronous host has no cancellation source. Retain nesting checks at its cleanup boundary.
void bray_runtime_cleanup_shield_enter(void)
{
    cleanup_shield_depth += 1;
    atomic_fetch_add_explicit(&cleanup_shield_entries, 1, memory_order_relaxed);
}

void bray_runtime_cleanup_shield_leave(void)
{
    if (cleanup_shield_depth == 0)
        abort();

    cleanup_shield_depth -= 1;
    atomic_fetch_add_explicit(&cleanup_shield_exits, 1, memory_order_relaxed);
}

extern uint32_t bray_runtime_initialization(uintptr_t worker_capacity, uintptr_t timer_capacity);
static uint32_t substrate_initialized;

uint32_t bray_runtime_substrate_initialization(uintptr_t worker_capacity, uintptr_t timer_capacity)
{
    (void)worker_capacity;
    (void)timer_capacity;

    if (substrate_initialized != 0)
        return 2;

    substrate_initialized = 1;

    return 0;
}

uint32_t bray_runtime_substrate_shutdown(void)
{
    if (substrate_initialized == 0)
        return 1;

    substrate_initialized = 0;

    return 0;
}

uint32_t bray_runtime_substrate_panic_reporting(
    uint32_t cause,
    uint32_t source_present,
    uint32_t source_identity,
    uint32_t source_start,
    uint32_t source_end,
    uint64_t source_version,
    const uint8_t* message,
    uintptr_t message_length,
    uint32_t (*copy_message)(const uint8_t*, uint8_t*, uintptr_t)
)
{
    (void)source_identity;
    (void)source_version;

    if (cause > 5 || source_present > 1 || source_start > source_end)
        return 3;

    if (message == NULL && message_length != 0)
        return 3;

    if (message_length != 0)
    {
        if (copy_message == NULL)
            return 3;

        uint8_t* copy = malloc(message_length);

        if (copy == NULL)
            return 5;

        uint32_t status = copy_message(message, copy, message_length);

        if (status == 0 && memcmp(copy, message, message_length) != 0)
            status = 3;

        free(copy);

        if (status != 0)
            return status;
    }

    return 0;
}

extern uintptr_t bray_runtime_panic_report_construction(
    uint32_t cause,
    uint32_t source_present,
    uint32_t source_identity,
    uint32_t source_start,
    uint32_t source_end,
    uint64_t source_version,
    const uint8_t* message,
    uintptr_t message_length
);
extern uint32_t bray_runtime_panic_reporting(uintptr_t report);
extern uint32_t bray_runtime_panic_report_observation(uintptr_t report);
extern uint32_t bray_runtime_panic_report_destruction(uintptr_t report);
extern uintptr_t bray_runtime_panic_report_suppression(uintptr_t primary, uintptr_t incident);
extern uintptr_t bray_runtime_cleanup_incident_construction(const CleanupIncident* incident);
extern uint32_t bray_runtime_structured_shutdown(void);

static uintptr_t panic_report(void)
{
    static const uint8_t message[] = "bootstrap panic";

    return bray_runtime_panic_report_construction(0, 0, 0, 0, 0, 0, message, sizeof(message) - 1);
}

typedef struct OwnedError
{
    uint32_t identity;
    uint32_t report_status;
} OwnedError;

static uint32_t owned_error_events[16];
static uint32_t owned_error_event_count;

static uint32_t report_owned_error(const CleanupIncident* incident)
{
    OwnedError* error = (OwnedError*)incident->payload;
    owned_error_events[owned_error_event_count++] = error->identity;

    return error->report_status;
}

static uintptr_t destroy_owned_error(uintptr_t payload)
{
    OwnedError* error = (OwnedError*)payload;
    uint32_t identity = error->identity;
    owned_error_events[owned_error_event_count++] = 100 + error->identity;

    free(error);

    return identity == 7 ? panic_report() : 0;
}

static PanicReportCallbacks panic_callbacks(void)
{
    PanicReportCallbacks callbacks = {
        &bray_runtime_panic_report_observation,
        &bray_runtime_panic_report_destruction,
        &bray_runtime_cleanup_incident_construction,
        &bray_runtime_panic_report_suppression,
    };

    return callbacks;
}

static uintptr_t owned_error_report(uint32_t identity, uint32_t report_status)
{
    OwnedError* error = malloc(sizeof(*error));

    if (error == NULL)
        abort();

    error->identity = identity;
    error->report_status = report_status;

    CleanupIncident incident = {0};

    incident.payload = (uintptr_t)error;
    incident.type_identity[0] = 7;
    incident.source.present = 1;
    incident.source.source = 9;
    incident.source.start = 2;
    incident.source.end = 5;
    incident.source.version = 11;
    incident.report = &report_owned_error;
    incident.destroy = &destroy_owned_error;
    incident.panics = panic_callbacks();

    return bray_runtime_cleanup_incident_construction(&incident);
}

static int verify_owned_cleanup_reports(void)
{
    uintptr_t primary = panic_report();

    uintptr_t report = bray_runtime_panic_report_suppression(primary, owned_error_report(1, 0));
    report = bray_runtime_panic_report_suppression(report, owned_error_report(2, 0));

    if (report != primary || bray_runtime_panic_reporting(report) != 0)
        return 0;

    report = bray_runtime_panic_report_suppression(panic_report(), owned_error_report(3, 0));

    if (bray_runtime_panic_report_destruction(report) != 0)
        return 0;

    report = bray_runtime_panic_report_suppression(owned_error_report(4, 5), owned_error_report(5, 0));

    if (bray_runtime_panic_reporting(report) != 5)
        return 0;

    report = owned_error_report(6, 0);

    if (
        bray_runtime_panic_report_observation(report) != 0 ||
        bray_runtime_panic_report_observation(report) != 0 ||
        bray_runtime_panic_report_destruction(report) != 0
    )
        return 0;

    if (bray_runtime_panic_reporting(owned_error_report(7, 0)) != 0)
        return 0;

    const uint32_t expected[] = {1, 2, 101, 102, 103, 4, 5, 104, 105, 6, 6, 106, 7, 107};

    if (owned_error_event_count != sizeof(expected) / sizeof(expected[0]))
        return 0;

    for (size_t index = 0; index < sizeof(expected) / sizeof(expected[0]); index++)
        if (owned_error_events[index] != expected[index])
            return 0;

    return 1;
}

void bray_runtime_panic_propagation(uintptr_t report)
{
    (void)report;

    abort();
}

int main(void)
{
    if (bray_runtime_initialization(1, 1) != 0 || bray_runtime_initialization(1, 1) != 2)
        return 1;

    uintptr_t pending = panic_report();

    if (bray_runtime_structured_shutdown() != 4)
        return 2;

    if (bray_runtime_panic_reporting(pending) != 0)
        return 3;

    if (bray_runtime_panic_report_destruction(panic_report()) != 0)
        return 4;

    for (uint32_t cause = 3; cause <= 5; ++cause)
    {
        uintptr_t allocation_failure = bray_runtime_panic_report_construction(cause, 0, 0, 0, 0, 0, NULL, 0);

        if (bray_runtime_panic_reporting(allocation_failure) != 0)
            return 5;
    }

    if (!verify_owned_cleanup_reports())
        return 6;

    if (bray_runtime_structured_shutdown() != 0 || bray_runtime_structured_shutdown() != 1)
        return 7;

    if (bray_runtime_initialization(1, 1) != 0 || bray_runtime_structured_shutdown() != 0)
        return 8;

    if (cleanup_shield_depth != 0 ||
        atomic_load_explicit(&cleanup_shield_entries, memory_order_relaxed) == 0 ||
        atomic_load_explicit(&cleanup_shield_entries, memory_order_relaxed) !=
            atomic_load_explicit(&cleanup_shield_exits, memory_order_relaxed))
        return 9;

    return 0;
}
