#ifndef BRAY_LLD_TELEMETRY_H
#define BRAY_LLD_TELEMETRY_H

#include "llvm/LTO/Config.h"
#include "llvm/Support/Caching.h"
#include "llvm/Support/Error.h"

namespace bray::lld
{

void configure_telemetry(llvm::lto::Config& config);

llvm::Expected<llvm::FileCache> telemetry_cache(
    const llvm::Twine& directory,
    llvm::AddBufferFn add_buffer
);

bool finish_telemetry();

void abandon_telemetry();

}

#endif
