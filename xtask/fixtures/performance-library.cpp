using u64 = __UINT64_TYPE__;

u64 accumulate(u64 seed, u64 count) noexcept
{
    u64 value = seed;

    while (count > 0)
    {
        --count;
        value += count;
    }

    return value;
}
