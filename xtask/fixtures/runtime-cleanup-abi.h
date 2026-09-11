#ifndef BRAY_RUNTIME_CLEANUP_ABI_H
#define BRAY_RUNTIME_CLEANUP_ABI_H

#include <stddef.h>
#include <stdint.h>

struct NativeFrameMetadata;
typedef struct CleanupIncident CleanupIncident;

typedef struct ProductHostObservation
{
    uint32_t status;
    uint32_t state;
    uintptr_t active_entries;
    uintptr_t external_roots;
    uintptr_t retirement_roots;
    uintptr_t thread_attachments;
    uintptr_t initialized_statics;
    uintptr_t cleaned_statics;
    uintptr_t cleanup_incidents;
    uint8_t last_incident[32];
} ProductHostObservation;

typedef struct SourceAnchor
{
    uint32_t present;
    uint32_t source;
    uint32_t start;
    uint32_t end;
    uint64_t version;
} SourceAnchor;

typedef struct PanicReportCallbacks
{
    uint32_t (*report)(uintptr_t);
    uint32_t (*destroy)(uintptr_t);
    uintptr_t (*construct_cleanup)(const CleanupIncident*);
    uintptr_t (*suppress)(uintptr_t, uintptr_t);
} PanicReportCallbacks;

struct CleanupIncident
{
    uintptr_t payload;
    uint8_t type_identity[32];
    SourceAnchor source;
    uint32_t (*report)(const CleanupIncident*);
    uintptr_t (*destroy)(uintptr_t);
    PanicReportCallbacks panics;
};

typedef struct StaticFinalizer
{
    uint32_t execution;
    uint32_t reserved;
    const struct NativeFrameMetadata* (*metadata)(void);
    uint32_t (*start)(uintptr_t, uintptr_t*);
    PanicReportCallbacks panics;
} StaticFinalizer;

_Static_assert(sizeof(ProductHostObservation) == 8 + 7 * sizeof(uintptr_t) + 32, "product observation layout");
_Static_assert(offsetof(ProductHostObservation, retirement_roots) == 8 + 2 * sizeof(uintptr_t), "provider retirement offset");
_Static_assert(sizeof(StaticFinalizer) == 8 + 6 * sizeof(uintptr_t), "static finalizer layout");
_Static_assert(offsetof(StaticFinalizer, panics) == 8 + 2 * sizeof(uintptr_t), "static finalizer panic callbacks offset");

#endif
