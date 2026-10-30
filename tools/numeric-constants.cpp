// Certify the bootstrap numeric constant floors with directed MPFR intervals.
// Build separately: c++ -O2 -std=c++20 tools/numeric-constants.cpp -lmpfr -lgmp
#include <cstdio>
#include <cstdlib>
#include <mpfr.h>

namespace
{

void certify(char const* family, unsigned int index, mpfr_srcptr lower, mpfr_srcptr upper, unsigned long scale)
{
    mpfr_t scaled_lower;
    mpfr_t scaled_upper;
    mpz_t integer_lower;
    mpz_t integer_upper;

    mpfr_inits2(2048, scaled_lower, scaled_upper, static_cast<mpfr_ptr>(nullptr));
    mpz_inits(integer_lower, integer_upper, static_cast<mpz_ptr>(nullptr));
    mpfr_abs(scaled_lower, lower, MPFR_RNDN);
    mpfr_abs(scaled_upper, upper, MPFR_RNDN);
    mpfr_mul_2ui(scaled_lower, scaled_lower, scale, MPFR_RNDN);
    mpfr_mul_2ui(scaled_upper, scaled_upper, scale, MPFR_RNDN);
    mpfr_get_z(integer_lower, scaled_lower, MPFR_RNDZ);
    mpfr_get_z(integer_upper, scaled_upper, MPFR_RNDZ);

    if (mpz_cmp(integer_lower, integer_upper) != 0)
    {
        std::exit(1);
    }

    char hex[300];

    mpz_get_str(hex, 16, integer_lower);
    std::printf("%s %u %s\n", family, index, hex);
    mpz_clears(integer_lower, integer_upper, static_cast<mpz_ptr>(nullptr));
    mpfr_clears(scaled_lower, scaled_upper, static_cast<mpfr_ptr>(nullptr));
}

}

int main()
{
    mpfr_t input;
    mpfr_t lower;
    mpfr_t upper;

    mpfr_inits2(2048, input, lower, upper, static_cast<mpfr_ptr>(nullptr));
    mpfr_const_pi(lower, MPFR_RNDD);
    mpfr_const_pi(upper, MPFR_RNDU);
    certify("pi", 0, lower, upper, 1022);
    mpfr_set_ui(input, 2, MPFR_RNDN);
    mpfr_log(lower, input, MPFR_RNDD);
    mpfr_log(upper, input, MPFR_RNDU);
    certify("ln2", 0, lower, upper, 1024);

    for (unsigned int index = 1; index < 32; ++index)
    {
        mpfr_set_ui(input, index, MPFR_RNDN);
        mpfr_div_2ui(input, input, 4, MPFR_RNDN);

        if (index < 16)
        {
            mpfr_atan(lower, input, MPFR_RNDD);
            mpfr_atan(upper, input, MPFR_RNDU);
            certify("atan", index, lower, upper, 1024);
        }

        if (index > 8 && index != 16)
        {
            mpfr_log(lower, input, MPFR_RNDD);
            mpfr_log(upper, input, MPFR_RNDU);
            certify("log", index, lower, upper, 1024);
        }
    }

    mpfr_clears(input, lower, upper, static_cast<mpfr_ptr>(nullptr));
}
