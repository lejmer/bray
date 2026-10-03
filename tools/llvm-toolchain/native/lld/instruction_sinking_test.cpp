#include "llvm/Config/llvm-config.h"
#undef LLVM_FORCE_ENABLE_STATS
#define LLVM_FORCE_ENABLE_STATS 1
#include "InstructionCombining.cpp"
#undef DEBUG_TYPE

#include "llvm/AsmParser/Parser.h"
#include "llvm/IR/Verifier.h"
#include "llvm/Passes/PassBuilder.h"
#include "llvm/Support/SourceMgr.h"

#include <cstdlib>
#include <vector>

namespace
{
enum class Barrier
{
    None,
    Before,
    After,
};

void require(bool condition, const char* message)
{
    if (!condition)
    {
        llvm::errs() << message << '\n';
        std::exit(1);
    }
}

std::string fixture(unsigned count, const std::string& attributes, Barrier barrier, bool canonical,
                    unsigned prefix = 1)
{
    const unsigned split = count / 2;
    const unsigned blocks = count - prefix + 1;
    const bool pure = attributes.find("memory(none)") != std::string::npos;
    std::vector<std::vector<unsigned>> calls(blocks + 1);

    for (unsigned call = 0; call < count; ++call)
    {
        unsigned destination = call < prefix ? 0 : call - prefix + 1;

        if (canonical)
        {
            destination = blocks;

            if (!pure && barrier != Barrier::None)
            {
                const unsigned prefix = split + (barrier == Barrier::After ? 1 : 0);

                if (call < prefix)
                    destination = split;
            }
        }

        calls[destination].push_back(call);
    }

    std::string source = "@flag = global i32 0\ndeclare i32 @step(i32, ptr noalias) " + attributes;
    source += "\ndefine i32 @chain(i32 %seed, ptr noalias %context) {\n";

    for (unsigned block = 0; block <= blocks; ++block)
    {
        source += "block" + std::to_string(block) + ":\n";

        if (!canonical && block == split && barrier == Barrier::Before)
            source += "  store i32 9, ptr @flag\n";

        for (unsigned call : calls[block])
        {
            const std::string argument = call == 0 ? "%seed" : "%v" + std::to_string(call - 1);

            source += "  %v" + std::to_string(call) + " = call i32 @step(i32 " + argument;
            source += ", ptr noalias " +
                      std::string(canonical && block == blocks ? "nonnull " : "") + "%context)\n";
        }

        if (block == split && barrier != Barrier::None && (canonical || barrier == Barrier::After))
            source += "  store i32 9, ptr @flag\n";

        if (block < blocks)
        {
            const std::string suffix = std::to_string(block);

            source += "  %state" + suffix + " = load i32, ptr %context, align 4\n";
            source += "  %offset" + suffix + " = add i32 %state" + suffix + ", -1\n";
            source += "  %failed" + suffix + " = icmp ult i32 %offset" + suffix + ", 2\n";
            source += "  br i1 %failed" + suffix + ", label %propagated, label %block";
            source += std::to_string(block + 1) + "\n";
        }
        else
            source += "  br label %return\n";
    }

    source += "propagated:\n  br label %return\nreturn:\n";
    source += "  %result = phi i32 [0, %propagated], [%v" + std::to_string(count - 1);
    source += ", %block" + std::to_string(blocks) + "]\n  ret i32 %result\n}\n";

    return source;
}

std::unique_ptr<llvm::Module> optimize(const std::string& source, llvm::LLVMContext& context)
{
    llvm::SMDiagnostic diagnostic;
    auto module = llvm::parseAssemblyString(source, diagnostic, context);

    if (!module)
    {
        diagnostic.print("instruction-sinking-test", llvm::errs());
        std::exit(1);
    }

    require(!llvm::verifyModule(*module, &llvm::errs()), "input LLVM module must verify");

    llvm::PassBuilder builder;
    llvm::LoopAnalysisManager loops;
    llvm::FunctionAnalysisManager functions;
    llvm::CGSCCAnalysisManager cgscc;
    llvm::ModuleAnalysisManager modules;

    builder.registerLoopAnalyses(loops);
    builder.registerFunctionAnalyses(functions);
    builder.registerCGSCCAnalyses(cgscc);
    builder.registerModuleAnalyses(modules);
    builder.crossRegisterProxies(loops, functions, cgscc, modules);

    llvm::ModulePassManager passes;
    auto error = builder.parsePassPipeline(passes, "function(instcombine)");

    require(!error, "InstCombine test pipeline must be available");
    passes.run(*module, modules);
    require(!llvm::verifyModule(*module, &llvm::errs()), "optimized LLVM module must verify");

    return module;
}

std::string printed(const llvm::Module& module)
{
    std::string result;
    llvm::raw_string_ostream output(result);

    module.print(output, nullptr);

    return result;
}

void checkChain(unsigned count, const std::string& attributes, Barrier barrier, unsigned prefix = 1)
{
    llvm::LLVMContext context;

    NumSunkInst = 0;
    NumSinkMemoryChecks = 0;

    auto actual = optimize(fixture(count, attributes, barrier, false, prefix), context);
    const auto moves = NumSunkInst.getValue();
    const auto memoryChecks = NumSinkMemoryChecks.getValue();
    auto expected = optimize(fixture(count, attributes, barrier, true, prefix), context);

    require(moves <= count * 2, "chain sinking must perform linear work");
    require(memoryChecks <= count * 12, "chain memory checks must perform linear work");
    require(moves != 0, "test-only LLVM work statistics must be enabled");
    const std::string actualIR = printed(*actual);
    const std::string expectedIR = printed(*expected);

    if (actualIR != expectedIR)
    {
        const auto mismatch =
            std::mismatch(actualIR.begin(), actualIR.end(), expectedIR.begin(), expectedIR.end());
        const auto offset = mismatch.first - actualIR.begin();

        llvm::errs() << "fixture: " << attributes << " count=" << count
                     << " barrier=" << static_cast<unsigned>(barrier) << '\n';
        llvm::errs() << "actual: " << actualIR.substr(offset, 160) << '\n';
        llvm::errs() << "expected: " << expectedIR.substr(offset, 160) << '\n';
    }

    require(actualIR == expectedIR, "batched sinking must preserve the optimized IR");
}

void checkAnchored(const std::string& attributes)
{
    llvm::LLVMContext context;

    NumSunkInst = 0;

    auto module = optimize(fixture(64, attributes, Barrier::None, false), context);

    require(NumSunkInst.getValue() == 0, "unsafe calls must stay in their original blocks");
}

void checkDeadLocalWriter()
{
    constexpr unsigned count = 1000;
    std::string source = fixture(count, "nounwind willreturn memory(read)", Barrier::None, false);

    source.insert(
        0,
        "declare i32 @writer(i32, ptr captures(none)) nounwind willreturn memory(argmem: write)\n");
    source.insert(source.find("block0:\n") + 8, "  %local = alloca i32, align 4\n");

    const std::string original = "call i32 @step(i32 %seed, ptr noalias %context)";

    source.replace(source.find(original), original.size(),
                   "call i32 @writer(i32 %seed, ptr %local)");

    llvm::LLVMContext context;

    NumSunkInst = 0;

    auto module = optimize(source, context);
    const auto moves = NumSunkInst.getValue();

    require(moves != 0 && moves <= count * 2,
            "dead-local producer chains must perform linear work");

    for (const llvm::BasicBlock& block : *module->getFunction("chain"))
        for (const llvm::Instruction& instruction : block)
            if (llvm::isa<llvm::CallBase>(instruction))
                require(block.getName() == "block1000",
                        "dead-local writers must follow their consumers");
}

void checkSinkAnchors(unsigned uses, bool assumption, bool successor)
{
    std::string source = fixture(8, "nounwind willreturn memory(read)", Barrier::None, false);
    std::string declarations = "declare ptr @makeptr(i32) nounwind willreturn memory(none)\n"
                               "declare i32 @consume(ptr, ptr) nounwind willreturn memory(read)\n"
                               "declare i32 @wide(...) nounwind willreturn memory(none)\n"
                               "declare void @llvm.assume(i1 noundef)\n";

    source.insert(0, declarations);
    source.insert(source.find("block0:\n") + 8, "  %pointer = call ptr @makeptr(i32 %seed)\n");

    const std::string original = "call i32 @step(i32 %seed, ptr noalias %context)";
    std::string consumer = "call i32 @consume(ptr %pointer, ptr %context)";

    if (!assumption)
    {
        consumer = "call i32 (...) @wide(";

        for (unsigned use = 0; use < uses; ++use)
            consumer += std::string(use ? ", " : "") + "ptr %pointer";

        consumer += ")";
    }

    source.replace(source.find(original), original.size(), consumer);

    const std::string anchor =
        "  call void @llvm.assume(i1 true) [\"dereferenceable\"(ptr %pointer, i64 4)]\n";

    if (successor)
    {
        const auto start = source.find("  %v0 =");
        const auto length = source.find('\n', start) + 1 - start;
        const std::string first = source.substr(start, length);

        source.erase(start, length);
        source.insert(source.find("block1:\n") + 8, anchor + first);
    }
    else if (assumption)
        source.insert(source.find("  %v0 ="), anchor);

    llvm::LLVMContext context;
    auto module = optimize(source, context);
    const std::string destination = assumption
                                        ? (successor ? "block1" : "block0")
                                        : (uses <= MaxSinkNumUsers + 1 ? "block8" : "block0");
    unsigned pointers = 0;
    unsigned assumes = 0;

    for (const llvm::BasicBlock& block : *module->getFunction("chain"))
        for (const llvm::Instruction& instruction : block)
            if (auto* call = llvm::dyn_cast<llvm::CallBase>(&instruction))
            {
                auto* function = call->getCalledFunction();

                if (function && function->getName() == "makeptr")
                {
                    ++pointers;
                    require(block.getName() == destination,
                            "sinking must preserve assume anchors and the existing use limit");
                }
                if (call->getIntrinsicID() == llvm::Intrinsic::assume)
                {
                    ++assumes;
                    require(block.getName() == destination,
                            "dereferenceable assumes must remain anchored");
                }
            }

    require(pointers == 1, "anchor fixture must retain its unknown pointer producer");
    require(assumes == static_cast<unsigned>(assumption),
            "dereferenceable assumes must be preserved");
}
} // namespace

int main()
{
    checkChain(1000, "nounwind willreturn memory(read)", Barrier::None);
    checkChain(1000, "nounwind willreturn memory(none)", Barrier::None);
    checkChain(1000, "nounwind willreturn memory(read)", Barrier::None, 500);
    checkChain(100, "nounwind willreturn memory(read)", Barrier::Before);
    checkChain(100, "nounwind willreturn memory(read)", Barrier::After);
    checkChain(100, "nounwind willreturn memory(none)", Barrier::Before);
    checkAnchored("willreturn memory(read)");
    checkAnchored("nounwind memory(read)");
    checkAnchored("nounwind willreturn memory(write)");
    checkAnchored("convergent nounwind willreturn memory(read)");
    checkDeadLocalWriter();
    checkSinkAnchors(1, true, false);
    checkSinkAnchors(1, true, true);
    checkSinkAnchors(32, false, false);
    checkSinkAnchors(33, false, false);
    checkSinkAnchors(34, false, false);

    llvm::DebugCounter::instance().push_back("instcombine-visit=0-1000000");

    llvm::LLVMContext context;

    NumSunkInst = 0;

    auto counted =
        optimize(fixture(32, "nounwind willreturn memory(read)", Barrier::None, false), context);

    require(NumSunkInst.getValue() == 32 * 33 / 2,
            "explicit visit counters must retain individual sinking");

    llvm::outs() << "instruction sinking tests passed\n";
}
