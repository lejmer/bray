#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct ProductHostObservation {
    uint32_t status;
    uint32_t state;
    uintptr_t active_entries;
    uintptr_t external_roots;
    uintptr_t thread_attachments;
    uintptr_t initialized_statics;
    uintptr_t cleaned_statics;
    uintptr_t cleanup_incidents;
    uint8_t last_incident[32];
} ProductHostObservation;

typedef struct SourceAnchor {
    uint32_t present;
    uint32_t source;
    uint32_t start;
    uint32_t end;
    uint64_t version;
} SourceAnchor;

typedef struct PanicReport PanicReport;
struct PanicReport {
    SourceAnchor source;
    uint32_t cause;
    uintptr_t message;
    uintptr_t message_length;
    uint32_t (*copy_message)(uintptr_t, uintptr_t, uint8_t *, uintptr_t);
    void (*release_message)(uintptr_t, uintptr_t);
    uintptr_t head;
    uintptr_t tail;
    uintptr_t count;
    uintptr_t reserved;
    uint32_t (*consume)(PanicReport *, _Bool);
};

typedef struct RunOutcome {
    uint32_t state;
    uintptr_t payload;
    PanicReport report;
} RunOutcome;

_Static_assert(sizeof(PanicReport) == 104, "native report layout");
_Static_assert(sizeof(RunOutcome) == 120, "native outcome layout");

// This isolated bootstrap fixture models successful cleanup only. The linked Rust
// smoke fixture exercises real record admission, failed reservation and incidents.
static _Atomic uintptr_t outgoing_credits;

void bray_runtime_outgoing_admission(uintptr_t count, RunOutcome *outcome) {
    atomic_fetch_add(&outgoing_credits, count);
    *outcome = (RunOutcome){0};
}

uintptr_t bray_runtime_outgoing_activation(void) {
    if (atomic_load(&outgoing_credits) == 0) {
        fputs("bootstrap cleanup has no admitted record\n", stderr);
        abort();
    }
    return 1;
}

void bray_runtime_outgoing_retirement(uintptr_t record, RunOutcome *outcome) {
    if (record != 1 || outcome->state == 2) {
        fputs("unexpected bootstrap cleanup incident\n", stderr);
        abort();
    }
}

void bray_runtime_outgoing_discharge(uintptr_t count) {
    if (atomic_fetch_sub(&outgoing_credits, count) < count) {
        fputs("bootstrap cleanup discharged an unadmitted owner\n", stderr);
        abort();
    }
}

extern uint32_t bray_runtime_initialization(uintptr_t worker_capacity, uintptr_t timer_capacity);
static uint32_t substrate_initialized;
static atomic_uint cleanup_shield_balance;

// The isolated bootstrap host has no cancellation delivery. Check that cleanup shields balance.
void bray_runtime_cleanup_shield_enter(void) {
    atomic_fetch_add_explicit(&cleanup_shield_balance, 1, memory_order_relaxed);
}

void bray_runtime_cleanup_shield_leave(void) {
    if (atomic_fetch_sub_explicit(&cleanup_shield_balance, 1, memory_order_relaxed) == 0) {
        abort();
    }
}

uint32_t bray_runtime_substrate_initialization(uintptr_t worker_capacity, uintptr_t timer_capacity) {
    (void)worker_capacity;
    (void)timer_capacity;

    if (substrate_initialized != 0) {
        return 2;
    }

    substrate_initialized = 1;
    return 0;
}

uint32_t bray_runtime_substrate_shutdown(void) {
    if (substrate_initialized == 0) {
        return 1;
    }

    substrate_initialized = 0;
    return 0;
}

ProductHostObservation bray_runtime_product_host_control(void *descriptor, uint32_t operation) {
    (void)descriptor;
    (void)operation;

    ProductHostObservation observation = {0};
    return observation;
}

void bray_runtime_current_run_cancellation_propagation(void) {
    abort();
}

// This isolated bootstrap test supplies only the primary consumer. Detached records
// and native bridges are exercised against the real runtime by runtime-smoke.rs.
static uint32_t consume_report(PanicReport *report, _Bool reporting) {
    if (report->head || report->tail || report->count || report->reserved) {
        abort();
    }
    uint32_t status = 0;
    if (reporting && (report->cause > 4 || report->source.present > 1 ||
        report->source.start > report->source.end ||
        (report->message_length != 0 && report->message == 0))) {
        status = 3;
    }
    if (report->release_message != NULL) {
        report->release_message(report->message, report->message_length);
    }
    *report = (PanicReport){0};
    return status;
}

uint32_t bray_runtime_substrate_panic_report_initialization(PanicReport *report) {
    report->consume = consume_report;
    return 0;
}

PanicReport bray_runtime_panic_report_suppression(PanicReport *primary, PanicReport *incident) {
    (void)primary;
    (void)incident;
    abort(); // This isolated primary-report fixture must not invoke detached-record operations.
}

uint32_t bray_runtime_panic_reporting(PanicReport *report) {
    return report->consume == NULL ? 0 : report->consume(report, 1);
}

uint32_t bray_runtime_panic_report_destruction(PanicReport *report) {
    return report->consume == NULL ? 0 : report->consume(report, 0);
}

extern PanicReport bray_runtime_panic_report_construction(
    uint32_t cause,
    uint32_t source_present,
    uint32_t source_identity,
    uint32_t source_start,
    uint32_t source_end,
    uint64_t source_version,
    const uint8_t *message,
    uintptr_t message_length
);
extern uint32_t bray_runtime_structured_shutdown(void);

static PanicReport panic_report(void) {
    static const uint8_t message[] = "bootstrap panic";

    return bray_runtime_panic_report_construction(
        0,
        0,
        0,
        0,
        0,
        0,
        message,
        sizeof(message) - 1
    );
}

void bray_runtime_panic_propagation(PanicReport *report) {
    (void)report;
    abort();
}

int main(void) {
    if (bray_runtime_initialization(1, 1) != 0 || bray_runtime_initialization(1, 1) != 2) {
        return 1;
    }

    PanicReport pending = panic_report();

    if (bray_runtime_structured_shutdown() != 4) {
        return 2;
    }

    if (bray_runtime_panic_reporting(&pending) != 0) {
        return 3;
    }

    PanicReport handled = panic_report();
    if (bray_runtime_panic_report_destruction(&handled) != 0) {
        return 4;
    }

    if (bray_runtime_structured_shutdown() != 0 || bray_runtime_structured_shutdown() != 1) {
        return 5;
    }

    if (bray_runtime_initialization(1, 1) != 0 || bray_runtime_structured_shutdown() != 0) {
        return 6;
    }

    if (atomic_load_explicit(&cleanup_shield_balance, memory_order_relaxed) != 0) {
        return 7;
    }

    if (atomic_load(&outgoing_credits) != 0) {
        return 8;
    }

    return 0;
}
