#ifndef BRAY_LLD_TELEMETRY_H
#define BRAY_LLD_TELEMETRY_H

#include "llvm/ADT/STLFunctionalExtras.h"
#include "llvm/IR/GlobalValue.h"
#include "llvm/LTO/Config.h"
#include "llvm/Support/Caching.h"
#include "llvm/Support/Error.h"

#include <cstdint>

namespace bray::lld
{

void definition_sizes(
    const llvm::Module& module,
    llvm::function_ref<void(const llvm::GlobalValue&, std::uint64_t)> record
);

void configure_telemetry(llvm::lto::Config& config);

llvm::Expected<llvm::FileCache> telemetry_cache(
    const llvm::Twine& directory,
    llvm::AddBufferFn add_buffer
);

bool finish_telemetry();

void abandon_telemetry();

}

#endif
