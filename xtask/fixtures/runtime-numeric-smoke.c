#include <inttypes.h>

typedef struct
{
    uint64_t words[2];
} NumericRealBits;

typedef struct
{
    NumericRealBits real;
    NumericRealBits imaginary;
} NumericComplexBits;

_Static_assert(sizeof(NumericRealBits) == 16, "real record size");
_Static_assert(_Alignof(NumericRealBits) == 8, "real record alignment");
_Static_assert(sizeof(NumericComplexBits) == 32, "complex record size");
_Static_assert(_Alignof(NumericComplexBits) == 8, "complex record alignment");
_Static_assert(offsetof(NumericComplexBits, imaginary) == 16, "imaginary component offset");

extern void bray_runtime_real_power(uint32_t width, const NumericRealBits* left,
    const NumericRealBits* right, NumericRealBits* result);
extern void bray_runtime_complex_multiply(uint32_t width, const NumericComplexBits* left,
    const NumericComplexBits* right, NumericComplexBits* result);
extern void bray_runtime_complex_divide(uint32_t width, const NumericComplexBits* left,
    const NumericComplexBits* right, NumericComplexBits* result);
extern void bray_runtime_complex_power(uint32_t width, const NumericComplexBits* left,
    const NumericComplexBits* right, NumericComplexBits* result);

static int check_numeric_runtime(void)
{
    const char* path = getenv("BRAY_NUMERIC_VECTORS");

    if (path == NULL)
    {
        return 80;
    }

    FILE* input = fopen(path, "rb");

    if (input == NULL)
    {
        return 81;
    }

    uint64_t row[14];
    size_t ordinal = 0;
    size_t count;

    while ((count = fread(row, sizeof(uint64_t), 14, input)) == 14)
    {
        NumericComplexBits left = {{{row[2], row[3]}}, {{row[4], row[5]}}};
        NumericComplexBits right = {{{row[6], row[7]}}, {{row[8], row[9]}}};
        NumericComplexBits result = {0};
        uint32_t width = (uint32_t)row[1];

        switch (row[0])
        {
            case 1:
                bray_runtime_complex_multiply(width, &left, &right, &result);
                break;
            case 2:
                bray_runtime_complex_divide(width, &left, &right, &result);
                break;
            case 3:
                bray_runtime_real_power(width, &left.real, &right.real, &result.real);
                break;
            case 4:
                bray_runtime_complex_power(width, &left, &right, &result);
                break;
            default:
                fclose(input);
                return 82;
        }

        if (result.real.words[0] != row[10] || result.real.words[1] != row[11] ||
            result.imaginary.words[0] != row[12] || result.imaginary.words[1] != row[13])
        {
            fprintf(stderr, "numeric case %zu operation %u width %u: %016" PRIx64 "%016" PRIx64
                " %016" PRIx64 "%016" PRIx64 "\n", ordinal, (unsigned)row[0], width,
                result.real.words[1], result.real.words[0], result.imaginary.words[1], result.imaginary.words[0]);
            fclose(input);
            return 83;
        }

        ordinal += 1;
    }

    int valid = count == 0 && feof(input) && ordinal != 0;

    fclose(input);

    if (valid)
    {
        printf("numeric_cases=%zu\n", ordinal);
    }

    return valid ? 0 : 84;
}
