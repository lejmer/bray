#include "llvm/Support/Errc.h"
#include "llvm/Support/MemoryBuffer.h"
#include "llvm/WindowsManifest/WindowsManifestMerger.h"

namespace llvm::windows_manifest
{

class WindowsManifestMerger::WindowsManifestMergerImpl
{
};

bool isAvailable()
{
    return false;
}

WindowsManifestMerger::WindowsManifestMerger() = default;

WindowsManifestMerger::~WindowsManifestMerger() = default;

Error WindowsManifestMerger::merge(MemoryBufferRef)
{
    return createStringError(errc::not_supported, "manifest input is unavailable");
}

std::unique_ptr<MemoryBuffer> WindowsManifestMerger::getMergedManifest()
{
    return nullptr;
}

}
