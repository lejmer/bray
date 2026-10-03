// Batch legal single-user chains while retaining the pinned LLVM safety and debug behavior.

STATISTIC(NumSinkMemoryChecks, "Number of memory barriers inspected while sinking instructions");

static bool isIgnoredSinkUser(User* User)
{
    if (!User->isDroppable())
        return false;

    auto* Intrinsic = cast<IntrinsicInst>(User);

    return Intrinsic->getIntrinsicID() != Intrinsic::assume ||
           !Intrinsic->getOperandBundle("dereferenceable");
}

static Instruction* getSingleSinkUser(Instruction* I)
{
    Instruction* SingleUser = nullptr;
    unsigned NumUsers = 0;

    for (Use& U : I->uses())
    {
        if (isIgnoredSinkUser(U.getUser()))
            continue;
        if (NumUsers > MaxSinkNumUsers)
            return nullptr;

        auto* User = cast<Instruction>(U.getUser());

        if (SingleUser && SingleUser != User)
            return nullptr;
        SingleUser = User;
        ++NumUsers;
    }

    return SingleUser;
}

static std::optional<BasicBlock*> getOptionalSinkBlockForInst(Instruction* I, DominatorTree& DT)
{
    if (!EnableCodeSinking)
        return std::nullopt;

    BasicBlock* BB = I->getParent();
    BasicBlock* UserParent = nullptr;
    unsigned NumUsers = 0;

    for (Use& U : I->uses())
    {
        User* User = U.getUser();
        if (isIgnoredSinkUser(User))
            continue;

        if (NumUsers > MaxSinkNumUsers)
            return std::nullopt;

        Instruction* UserInst = cast<Instruction>(User);
        // Special handling for Phi nodes - get the block the use occurs in.
        BasicBlock* UserBB = UserInst->getParent();
        if (PHINode* PN = dyn_cast<PHINode>(UserInst))
            UserBB = PN->getIncomingBlock(U);
        // Bail out if we have uses in different blocks. We don't do any
        // sophisticated analysis (i.e finding NearestCommonDominator of these
        // use blocks).
        if (UserParent && UserParent != UserBB)
            return std::nullopt;
        UserParent = UserBB;

        // Make sure these checks are done only once, naturally we do the checks
        // the first time we get the userparent, this will save compile time.
        if (NumUsers == 0)
        {
            // Try sinking to another block. If that block is unreachable, then do
            // not bother. SimplifyCFG should handle it.
            if (UserParent == BB || !DT.isReachableFromEntry(UserParent))
                return std::nullopt;

            auto* Term = UserParent->getTerminator();
            // See if the user is one of our successors that has only one
            // predecessor, so that we don't have to split the critical edge.
            // Another option where we can sink is a block that ends with a
            // terminator that does not pass control to other block (such as
            // return or unreachable or resume). In this case:
            //   - I dominates the User (by SSA form).
            //   - the User will be executed at most once.
            // So sinking I down to User is always profitable or neutral.
            if (UserParent->getUniquePredecessor() != BB && !succ_empty(Term))
                return std::nullopt;

            assert(DT.dominates(BB, UserParent) && "Dominance relation broken?");
        }

        NumUsers++;
    }

    // No user or only has droppable users.
    if (!UserParent)
        return std::nullopt;

    return UserParent;
}

static bool isSinkableInstruction(Instruction* I, BasicBlock* DestBlock, TargetLibraryInfo& TLI)
{
    // Cannot move control-flow-involving, volatile loads, vaarg, etc.
    if (isa<PHINode>(I) || I->isEHPad() || I->mayThrow() || !I->willReturn() || I->isTerminator())
        return false;

    // Do not sink static or dynamic alloca instructions. Static allocas must
    // remain in the entry block, and dynamic allocas must not be sunk in between
    // a stacksave / stackrestore pair, which would incorrectly shorten its
    // lifetime.
    if (isa<AllocaInst>(I))
        return false;

    // Do not sink into catchswitch blocks.
    if (isa<CatchSwitchInst>(DestBlock->getTerminator()))
        return false;

    // Do not sink convergent call instructions.
    if (auto* CI = dyn_cast<CallInst>(I))
    {
        if (CI->isConvergent())
            return false;
    }

    // Unless we can prove that the memory write isn't visibile except on the
    // path we're sinking to, we must bail.
    if (I->mayWriteToMemory())
    {
        if (!SoleWriteToDeadLocal(I, TLI))
            return false;
    }

    return true;
}

static bool canSinkInstruction(Instruction* I, BasicBlock* DestBlock, TargetLibraryInfo& TLI)
{
    if (!isSinkableInstruction(I, DestBlock, TLI))
        return false;

    // We can only sink load instructions if there is nothing between the load and
    // the end of block that could change the value.
    if (I->mayReadFromMemory() && !I->hasMetadata(LLVMContext::MD_invariant_load))
    {
        // We don't want to do any sophisticated alias analysis, so we only check
        // the instructions after I in I's parent block if we try to sink to its
        // successor block.
        if (DestBlock->getUniquePredecessor() != I->getParent())
            return false;
        for (BasicBlock::iterator Scan = std::next(I->getIterator()), E = I->getParent()->end();
             Scan != E; ++Scan)
        {
            ++NumSinkMemoryChecks;
            if (Scan->mayWriteToMemory())
                return false;
        }
    }

    return true;
}

bool InstCombinerImpl::tryToSinkInstruction(Instruction* I, BasicBlock* DestBlock)
{
    if (!canSinkInstruction(I, DestBlock, TLI))
        return false;

    BasicBlock* OriginalDest = DestBlock;
    SmallPtrSet<Instruction*, 16> Members;
    Members.insert(I);
    // Selected writers already satisfy SoleWriteToDeadLocal and move in this batch.
    // Only stationary writes can block the remaining producer reads.
    DenseMap<BasicBlock*, Instruction*> LastWrites;
    auto LastWrite = [&](BasicBlock* Block)
    {
        auto [Position, Inserted] = LastWrites.try_emplace(Block, nullptr);
        if (Inserted)
            for (Instruction& Inst : *Block)
            {
                ++NumSinkMemoryChecks;
                if (Inst.mayWriteToMemory() && !Members.contains(&Inst))
                    Position->second = &Inst;
            }
        return Position->second;
    };
    SmallVector<Instruction*, 8> Chain{I};
    // Explicit visit counters must continue to control one instruction at a time.
    if (!DebugCounter::isCounterSet(VisitCounter))
    {
        while (auto* User = getSingleSinkUser(Chain.back()))
        {
            if (User->getParent() != DestBlock)
                break;
            auto NextBlock = getOptionalSinkBlockForInst(User, DT);
            if (!NextBlock || (*NextBlock)->getUniquePredecessor() != DestBlock ||
                !canSinkInstruction(User, *NextBlock, TLI))
                break;
            if (LastWrite(DestBlock) != nullptr)
                break;
            Chain.push_back(User);
            Members.insert(User);
            DestBlock = *NextBlock;
        }
    }

    if (Chain.size() > 1)
    {
        SmallVector<Instruction*, 8> Prefix;
        Instruction* First = I;
        BasicBlock* FirstDest = OriginalDest;
        bool Blocked = false;
        bool LaterWrites = false;
        while (true)
        {
            Instruction* Producer = nullptr;
            for (Value* Operand : First->operand_values())
            {
                auto* Candidate = dyn_cast<Instruction>(Operand);
                if (!Candidate)
                    continue;
                BasicBlock* Source = Candidate->getParent();
                BasicBlock* FirstBlock = First->getParent();
                if (Source != FirstBlock && FirstBlock->getUniquePredecessor() != Source)
                    continue;
                BasicBlock* CheckDest = Source == FirstBlock ? FirstDest : FirstBlock;
                if (!isSinkableInstruction(Candidate, CheckDest, TLI))
                    continue;
                if (Candidate->mayReadFromMemory() &&
                    !Candidate->hasMetadata(LLVMContext::MD_invariant_load))
                {
                    Instruction* Write = LastWrite(Source);
                    if (LaterWrites || (Write && Candidate->comesBefore(Write)) ||
                        (Source != FirstBlock && LastWrite(FirstBlock)))
                        continue;
                }
                if (getSingleSinkUser(Candidate) != First || (Producer && Producer != Candidate))
                {
                    Blocked = true;
                    break;
                }
                Producer = Candidate;
            }
            if (!Producer || Blocked)
                break;
            if (Producer->getParent() != First->getParent())
            {
                LaterWrites |= LastWrite(First->getParent()) != nullptr;
                FirstDest = First->getParent();
            }
            Prefix.push_back(Producer);
            Members.insert(Producer);
            auto Write = LastWrites.find(Producer->getParent());

            if (Write != LastWrites.end() && Write->second == Producer)
            {
                Write->second = nullptr;

                for (auto Scan = Producer->getIterator(); Scan != Producer->getParent()->begin();)
                {
                    --Scan;
                    ++NumSinkMemoryChecks;

                    if (Scan->mayWriteToMemory() && !Members.contains(&*Scan))
                    {
                        Write->second = &*Scan;
                        break;
                    }
                }
            }
            First = Producer;
        }
        for (Instruction* Node : Chain)
        {
            if (Node == I)
                continue;
            for (Value* Operand : Node->operand_values())
            {
                auto* Candidate = dyn_cast<Instruction>(Operand);
                if (Candidate && !Members.contains(Candidate) &&
                    Candidate->getParent() == Node->getParent() &&
                    isSinkableInstruction(Candidate, DestBlock, TLI))
                    Blocked = true;
            }
        }
        if (Blocked)
        {
            Chain.clear();
            Chain.push_back(I);
            DestBlock = OriginalDest;
        }
        else
        {
            Chain.insert(Chain.begin(), Prefix.rbegin(), Prefix.rend());
        }
    }

    auto Move = [&](Instruction* I)
    {
        BasicBlock* SrcBlock = I->getParent();
        I->dropDroppableUses(
            [&](const Use* U)
            {
                auto* I = dyn_cast<Instruction>(U->getUser());
                if (I && I->getParent() != DestBlock)
                {
                    Worklist.add(I);
                    return true;
                }
                return false;
            });
        /// FIXME: We could remove droppable uses that are not dominated by
        /// the new position.

        BasicBlock::iterator InsertPos = DestBlock->getFirstInsertionPt();
        I->moveBefore(*DestBlock, InsertPos);
        ++NumSunkInst;

        // Also sink all related debug uses from the source basic block. Otherwise we
        // get debug use before the def. Attempt to salvage debug uses first, to
        // maximise the range variables have location for. If we cannot salvage, then
        // mark the location undef: we know it was supposed to receive a new location
        // here, but that computation has been sunk.
        SmallVector<DbgVariableRecord*, 2> DbgVariableRecords;
        findDbgUsers(I, DbgVariableRecords);
        if (!DbgVariableRecords.empty())
            tryToSinkInstructionDbgVariableRecords(I, InsertPos, SrcBlock, DestBlock,
                                                   DbgVariableRecords);

        // PS: there are numerous flaws with this behaviour, not least that right now
        // assignments can be re-ordered past other assignments to the same variable
        // if they use different Values. Creating more undef assignements can never be
        // undone. And salvaging all users outside of this block can un-necessarily
        // alter the lifetime of the live-value that the variable refers to.
        // Some of these things can be resolved by tolerating debug use-before-defs in
        // LLVM-IR, however it depends on the instruction-referencing CodeGen backend
        // being used for more architectures.
    };
    for (Instruction* Moved : llvm::reverse(Chain))
    {
        Move(Moved);
        if (Moved != I)
        {
            Worklist.push(Moved);
            for (Use& Operand : Moved->operands())
                if (auto* Producer = dyn_cast<Instruction>(Operand.get()))
                    Worklist.push(Producer);
        }
    }
    return true;
}
