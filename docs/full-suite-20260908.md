# Fresh complete-suite benchmark, 2026-09-08

Updated 2026-09-16T18:15:39.713742+00:00. Controller status: **completed_with_failures_and_skipped_inputs**.

191 unique cases: 168 runnable backend cases and 23 graph inputs
skipped by user request pending SubTropica frontend preparation. Each runnable case is
scheduled at one and eight workers, with three fresh timed pairs.
Completed case/mode combinations: **332/336**; failed: **4**.

Single sequential queue: no overlapping suite jobs. Same eight physical cores, one-core mode restricted to one of those cores. Three timed pairs per case/mode; backend and mode order alternate. One warmup pair except four very expensive cases. Timings include process startup. Shared host; CPU, memory and scheduler-wait traces retained. A failed backend is skipped for subsequent repetitions of that case/mode, and the suite continues.

CPU set: `[1, 2, 3, 5, 6, 7, 8, 9]`; serial CPU: `1`.
Every invocation allows 256 GiB address space, 256 MiB stacks and
24 hours. Hard cases tst3, tst4 and both qboxes omit the warmup.
All measurements use the pinned scratch-fixed build plus the
previous integration-memory and rolling-queue changes.

Times below are medians of accepted timed samples. Incomplete rows
remain marked incomplete; failed or blocked rows have no speed ratio.
CPU utilization is process CPU time divided by wall time and worker budget.

| Case | Workers | Status | Pairs | FLINT s | Symbolica s | FLINT / Symbolica | Peak MiB FLINT / Symbolica |
|---|---:|---|---:|---:|---:|---:|---:|
| benchmark.tiny_rational_addition | 1 | measured | 3 | 0.019 | 0.006 | 2.941 | 12.066 / 12.094 |
| benchmark.tiny_rational_addition | 8 | measured | 3 | 0.018 | 0.006 | 3.064 | 12.047 / 12.059 |
| benchmark.sparse_high_degree_multiply | 1 | measured | 3 | 0.019 | 0.006 | 2.937 | 12.094 / 12.070 |
| benchmark.sparse_high_degree_multiply | 8 | measured | 3 | 0.019 | 0.006 | 3.143 | 12.059 / 12.090 |
| benchmark.dense_polynomial_multiply | 1 | measured | 3 | 0.019 | 0.006 | 2.998 | 12.062 / 12.078 |
| benchmark.dense_polynomial_multiply | 8 | measured | 3 | 0.017 | 0.006 | 3.035 | 12.078 / 12.078 |
| benchmark.multivariate_gcd | 1 | measured | 3 | 0.020 | 0.007 | 2.960 | 12.094 / 12.070 |
| benchmark.multivariate_gcd | 8 | measured | 3 | 0.018 | 0.006 | 2.867 | 12.062 / 12.094 |
| benchmark.dense_parameter_resultant | 1 | measured | 3 | 2.969 | 0.486 | 6.105 | 43.551 / 67.473 |
| benchmark.dense_parameter_resultant | 8 | measured | 3 | 2.996 | 0.485 | 6.183 | 43.551 / 67.465 |
| benchmark.shared_denominator_rat_sum | 1 | measured | 3 | 0.020 | 0.009 | 2.160 | 12.078 / 12.062 |
| benchmark.shared_denominator_rat_sum | 8 | measured | 3 | 0.019 | 0.008 | 2.233 | 12.078 / 12.062 |
| benchmark.large_multiplicity_partial_fractions | 1 | measured | 3 | 0.083 | 0.039 | 2.132 | 30.051 / 19.492 |
| benchmark.large_multiplicity_partial_fractions | 8 | measured | 3 | 0.082 | 0.037 | 2.222 | 30.121 / 19.480 |
| benchmark.factor_table_multistage | 1 | measured | 3 | 0.020 | 0.007 | 2.937 | 12.066 / 12.094 |
| benchmark.factor_table_multistage | 8 | measured | 3 | 0.019 | 0.007 | 2.844 | 12.074 / 12.070 |
| benchmark.lr_verify_three_variables | 1 | measured | 3 | 0.022 | 0.008 | 2.794 | 12.062 / 12.066 |
| benchmark.lr_verify_three_variables | 8 | measured | 3 | 0.021 | 0.007 | 2.906 | 12.055 / 12.090 |
| benchmark.euler_filtered_massless_box | 1 | measured | 3 | 0.022 | 0.008 | 2.885 | 12.070 / 12.094 |
| benchmark.euler_filtered_massless_box | 8 | measured | 3 | 0.022 | 0.007 | 3.171 | 12.059 / 12.094 |
| benchmark.high_order_laurent_series | 1 | measured | 3 | 0.025 | 0.007 | 3.782 | 12.090 / 12.062 |
| benchmark.high_order_laurent_series | 8 | measured | 3 | 0.025 | 0.007 | 3.625 | 12.078 / 12.051 |
| benchmark.production_mzv_reduction | 1 | measured | 3 | 0.028 | 0.010 | 2.918 | 15.051 / 12.094 |
| benchmark.production_mzv_reduction | 8 | measured | 3 | 0.028 | 0.009 | 3.146 | 15.035 / 12.062 |
| benchmark.convergent_integration_step | 1 | measured | 3 | 0.042 | 0.008 | 5.320 | 22.246 / 12.070 |
| benchmark.convergent_integration_step | 8 | measured | 3 | 0.041 | 0.006 | 7.207 | 22.285 / 12.047 |
| benchmark.three_variable_hyperflint | 1 | measured | 3 | 0.025 | 0.006 | 4.173 | 18.062 / 12.066 |
| benchmark.three_variable_hyperflint | 8 | measured | 3 | 0.029 | 0.009 | 3.140 | 18.020 / 12.062 |
| api.mul_difference_of_squares | 1 | measured | 3 | 0.019 | 0.006 | 2.989 | 12.094 / 12.090 |
| api.mul_difference_of_squares | 8 | measured | 3 | 0.018 | 0.006 | 2.865 | 12.051 / 12.066 |
| api.gcd_shared_linear_factor | 1 | measured | 3 | 0.019 | 0.007 | 2.841 | 12.078 / 12.078 |
| api.gcd_shared_linear_factor | 8 | measured | 3 | 0.019 | 0.006 | 3.153 | 12.047 / 9.055 |
| api.resultant_linear_substitution | 1 | measured | 3 | 0.020 | 0.006 | 3.056 | 12.062 / 12.062 |
| api.resultant_linear_substitution | 8 | measured | 3 | 0.019 | 0.006 | 3.025 | 12.070 / 9.043 |
| api.hyperflint_discriminant_sign | 1 | measured | 3 | 0.020 | 0.007 | 2.970 | 12.094 / 12.094 |
| api.hyperflint_discriminant_sign | 8 | measured | 3 | 0.020 | 0.006 | 3.111 | 12.066 / 12.066 |
| api.rational_addition | 1 | measured | 3 | 0.020 | 0.007 | 3.064 | 12.090 / 12.066 |
| api.rational_addition | 8 | measured | 3 | 0.019 | 0.006 | 2.967 | 12.078 / 12.070 |
| api.rational_cross_cancellation | 1 | measured | 3 | 0.020 | 0.007 | 2.927 | 12.066 / 12.078 |
| api.rational_cross_cancellation | 8 | measured | 3 | 0.019 | 0.007 | 2.920 | 12.090 / 12.090 |
| api.rational_substitution | 1 | measured | 3 | 0.019 | 0.007 | 2.904 | 12.062 / 12.062 |
| api.rational_substitution | 8 | measured | 3 | 0.020 | 0.007 | 2.948 | 12.066 / 12.094 |
| api.two_letter_shuffle | 1 | measured | 3 | 0.020 | 0.007 | 2.929 | 12.070 / 12.094 |
| api.two_letter_shuffle | 8 | measured | 3 | 0.019 | 0.006 | 3.027 | 12.062 / 12.031 |
| api.expression_parser | 1 | measured | 3 | 0.014 | 0.006 | 2.385 | 12.066 / 12.066 |
| api.expression_parser | 8 | measured | 3 | 0.019 | 0.006 | 3.006 | 12.039 / 12.062 |
| api.parser_unary_minus_power_precedence | 1 | measured | 3 | 0.017 | 0.006 | 2.862 | 12.090 / 12.078 |
| api.parser_unary_minus_power_precedence | 8 | measured | 3 | 0.019 | 0.006 | 2.982 | 12.078 / 12.066 |
| api.parser_parenthesized_negative_base | 1 | measured | 3 | 0.020 | 0.006 | 3.126 | 12.066 / 12.078 |
| api.parser_parenthesized_negative_base | 8 | measured | 3 | 0.019 | 0.006 | 3.207 | 12.094 / 12.066 |
| api.parser_right_associative_power | 1 | measured | 3 | 0.019 | 0.007 | 2.885 | 12.059 / 12.090 |
| api.parser_right_associative_power | 8 | measured | 3 | 0.020 | 0.006 | 3.182 | 12.066 / 9.059 |
| api.empty_hlog_word | 1 | measured | 3 | 0.015 | 0.005 | 3.178 | 12.066 / 12.066 |
| api.empty_hlog_word | 8 | measured | 3 | 0.018 | 0.006 | 3.044 | 12.062 / 12.062 |
| api.repeated_linear_factorization | 1 | measured | 3 | 0.014 | 0.005 | 2.825 | 12.062 / 12.070 |
| api.repeated_linear_factorization | 8 | measured | 3 | 0.018 | 0.006 | 3.011 | 12.066 / 12.047 |
| api.repeated_pole_partial_fractions | 1 | measured | 3 | 0.014 | 0.005 | 2.741 | 13.941 / 12.070 |
| api.repeated_pole_partial_fractions | 8 | measured | 3 | 0.017 | 0.006 | 2.995 | 12.078 / 12.059 |
| api.simple_primitive | 1 | measured | 3 | 0.015 | 0.005 | 3.004 | 12.062 / 12.090 |
| api.simple_primitive | 8 | measured | 3 | 0.017 | 0.006 | 2.996 | 13.898 / 12.039 |
| api.linear_reducibility | 1 | measured | 3 | 0.014 | 0.005 | 2.864 | 12.078 / 12.094 |
| api.linear_reducibility | 8 | measured | 3 | 0.018 | 0.006 | 3.197 | 12.094 / 12.066 |
| api.verify_order_all_linear_multigroup | 1 | measured | 3 | 0.014 | 0.004 | 3.008 | 12.066 / 12.078 |
| api.verify_order_all_linear_multigroup | 8 | measured | 3 | 0.013 | 0.005 | 2.534 | 12.059 / 12.066 |
| api.verify_order_genuine_quadratic_block | 1 | measured | 3 | 0.013 | 0.005 | 2.790 | 12.094 / 12.062 |
| api.verify_order_genuine_quadratic_block | 8 | measured | 3 | 0.018 | 0.006 | 3.071 | 12.031 / 12.070 |
| api.verify_order_malformed_permutation | 1 | measured | 3 | 0.014 | 0.004 | 3.337 | 12.066 / 12.070 |
| api.verify_order_malformed_permutation | 8 | measured | 3 | 0.015 | 0.005 | 2.883 | 12.070 / 12.047 |
| api.verify_order_forbidden_dependency_carry_inert | 1 | measured | 3 | 0.013 | 0.005 | 2.942 | 12.062 / 12.062 |
| api.verify_order_forbidden_dependency_carry_inert | 8 | measured | 3 | 0.018 | 0.004 | 4.090 | 12.066 / 12.055 |
| api.verify_order_e26_intersection_regression | 1 | measured | 3 | 0.097 | 0.042 | 2.314 | 13.961 / 18.551 |
| api.verify_order_e26_intersection_regression | 8 | measured | 3 | 0.097 | 0.058 | 1.674 | 13.934 / 18.480 |
| api.mzv_reduction | 1 | measured | 3 | 0.020 | 0.005 | 3.689 | 15.027 / 12.090 |
| api.mzv_reduction | 8 | measured | 3 | 0.027 | 0.007 | 3.743 | 15.020 / 12.062 |
| api.convergent_integration_step | 1 | measured | 3 | 0.029 | 0.006 | 5.023 | 22.301 / 12.066 |
| api.convergent_integration_step | 8 | measured | 3 | 0.041 | 0.006 | 7.148 | 22.293 / 12.051 |
| api.two_variable_hyperflint | 1 | measured | 3 | 0.020 | 0.007 | 3.042 | 18.020 / 12.090 |
| api.two_variable_hyperflint | 8 | measured | 3 | 0.024 | 0.008 | 2.873 | 18.027 / 12.039 |
| api.dispatch_add | 1 | measured | 3 | 0.015 | 0.004 | 3.353 | 12.066 / 12.090 |
| api.dispatch_add | 8 | measured | 3 | 0.016 | 0.005 | 2.852 | 12.078 / 12.066 |
| api.dispatch_algebraic_letters_allocate | 1 | measured | 3 | 0.016 | 0.005 | 3.386 | 12.062 / 12.066 |
| api.dispatch_algebraic_letters_allocate | 8 | measured | 3 | 0.018 | 0.006 | 3.103 | 12.070 / 12.094 |
| api.dispatch_algebraic_letters_clear | 1 | measured | 3 | 0.012 | 0.005 | 2.665 | 12.094 / 12.094 |
| api.dispatch_algebraic_letters_clear | 8 | measured | 3 | 0.017 | 0.005 | 3.621 | 12.094 / 12.062 |
| api.dispatch_algebraic_letters_show | 1 | measured | 3 | 0.015 | 0.004 | 4.008 | 12.078 / 12.062 |
| api.dispatch_algebraic_letters_show | 8 | measured | 3 | 0.015 | 0.005 | 3.122 | 12.094 / 12.062 |
| api.dispatch_back_substitute | 1 | measured | 3 | 0.013 | 0.005 | 2.811 | 12.059 / 12.070 |
| api.dispatch_back_substitute | 8 | measured | 3 | 0.015 | 0.005 | 3.131 | 12.059 / 12.078 |
| api.dispatch_break_up_contour_sym | 1 | measured | 3 | 0.020 | 0.005 | 4.073 | 14.992 / 12.062 |
| api.dispatch_break_up_contour_sym | 8 | measured | 3 | 0.023 | 0.005 | 4.554 | 14.988 / 12.062 |
| api.dispatch_collect_words | 1 | measured | 3 | 0.013 | 0.004 | 2.990 | 12.051 / 12.066 |
| api.dispatch_collect_words | 8 | measured | 3 | 0.017 | 0.005 | 3.208 | 12.094 / 12.078 |
| api.dispatch_combine_wm_wp_ratios | 1 | measured | 3 | 0.013 | 0.004 | 2.979 | 12.070 / 12.062 |
| api.dispatch_combine_wm_wp_ratios | 8 | measured | 3 | 0.013 | 0.004 | 3.001 | 12.066 / 12.090 |
| api.dispatch_concat_mul | 1 | measured | 3 | 0.013 | 0.004 | 2.959 | 12.066 / 12.078 |
| api.dispatch_concat_mul | 8 | measured | 3 | 0.013 | 0.005 | 2.644 | 12.090 / 12.047 |
| api.dispatch_convert_1inf_to_01 | 1 | measured | 3 | 0.013 | 0.004 | 2.829 | 12.094 / 12.094 |
| api.dispatch_convert_1inf_to_01 | 8 | measured | 3 | 0.014 | 0.004 | 3.183 | 12.047 / 12.094 |
| api.dispatch_convert_ab_to_zero_inf | 1 | measured | 3 | 0.013 | 0.004 | 2.965 | 12.047 / 12.055 |
| api.dispatch_convert_ab_to_zero_inf | 8 | measured | 3 | 0.013 | 0.005 | 2.459 | 12.062 / 12.094 |
| api.dispatch_derivative | 1 | measured | 3 | 0.012 | 0.004 | 2.846 | 12.094 / 12.047 |
| api.dispatch_derivative | 8 | measured | 3 | 0.012 | 0.004 | 2.790 | 12.059 / 12.094 |
| api.dispatch_differentiate_wordlist | 1 | measured | 3 | 0.013 | 0.004 | 2.854 | 12.066 / 12.070 |
| api.dispatch_differentiate_wordlist | 8 | measured | 3 | 0.018 | 0.005 | 3.278 | 12.059 / 12.039 |
| api.dispatch_diff_hlog | 1 | measured | 3 | 0.013 | 0.004 | 3.018 | 12.062 / 12.066 |
| api.dispatch_diff_hlog | 8 | measured | 3 | 0.014 | 0.005 | 3.021 | 12.078 / 12.062 |
| api.dispatch_diff_mpl | 1 | measured | 3 | 0.013 | 0.005 | 2.884 | 12.062 / 12.094 |
| api.dispatch_diff_mpl | 8 | measured | 3 | 0.015 | 0.005 | 3.058 | 12.090 / 12.090 |
| api.dispatch_divexact | 1 | measured | 3 | 0.013 | 0.005 | 2.867 | 12.062 / 12.078 |
| api.dispatch_divexact | 8 | measured | 3 | 0.013 | 0.004 | 3.005 | 12.062 / 12.066 |
| api.dispatch_eval | 1 | measured | 3 | 0.014 | 0.004 | 3.302 | 12.066 / 12.094 |
| api.dispatch_eval | 8 | measured | 3 | 0.016 | 0.005 | 3.441 | 12.090 / 12.066 |
| api.dispatch_expand_inf_word | 1 | measured | 3 | 0.013 | 0.004 | 2.991 | 12.070 / 12.062 |
| api.dispatch_expand_inf_word | 8 | measured | 3 | 0.013 | 0.004 | 3.267 | 12.066 / 12.059 |
| api.dispatch_expand_zero_word | 1 | measured | 3 | 0.013 | 0.004 | 3.221 | 12.094 / 12.094 |
| api.dispatch_expand_zero_word | 8 | measured | 3 | 0.013 | 0.004 | 3.195 | 12.090 / 12.047 |
| api.dispatch_find_lr_orders_scan | 1 | measured | 3 | 0.013 | 0.004 | 2.912 | 12.078 / 12.078 |
| api.dispatch_find_lr_orders_scan | 8 | measured | 3 | 0.013 | 0.004 | 2.993 | 12.090 / 12.078 |
| api.dispatch_hlog_series | 1 | measured | 3 | 0.013 | 0.005 | 2.792 | 12.090 / 12.066 |
| api.dispatch_hlog_series | 8 | measured | 3 | 0.015 | 0.004 | 3.508 | 12.090 / 12.070 |
| api.dispatch_hlog_zero_expand | 1 | measured | 3 | 0.013 | 0.005 | 2.751 | 12.066 / 12.094 |
| api.dispatch_hlog_zero_expand | 8 | measured | 3 | 0.012 | 0.005 | 2.505 | 12.066 / 12.094 |
| api.dispatch_mpl_series | 1 | measured | 3 | 0.020 | 0.006 | 3.182 | 12.090 / 12.090 |
| api.dispatch_mpl_series | 8 | measured | 3 | 0.021 | 0.006 | 3.402 | 12.051 / 12.066 |
| api.dispatch_mpl_sum | 1 | measured | 3 | 0.022 | 0.007 | 3.261 | 12.070 / 12.066 |
| api.dispatch_mpl_sum | 8 | measured | 3 | 0.020 | 0.007 | 3.044 | 12.078 / 12.055 |
| api.dispatch_neg | 1 | measured | 3 | 0.020 | 0.007 | 2.888 | 12.059 / 12.078 |
| api.dispatch_neg | 8 | measured | 3 | 0.020 | 0.006 | 3.015 | 12.066 / 12.051 |
| api.dispatch_pole_degree | 1 | measured | 3 | 0.019 | 0.007 | 2.867 | 12.094 / 12.090 |
| api.dispatch_pole_degree | 8 | measured | 3 | 0.020 | 0.007 | 2.852 | 12.090 / 12.094 |
| api.dispatch_rat_div | 1 | measured | 3 | 0.018 | 0.006 | 2.869 | 12.094 / 12.094 |
| api.dispatch_rat_div | 8 | measured | 3 | 0.018 | 0.006 | 3.051 | 12.094 / 12.051 |
| api.dispatch_rat_residue | 1 | measured | 3 | 0.019 | 0.006 | 2.979 | 12.094 / 12.090 |
| api.dispatch_rat_residue | 8 | measured | 3 | 0.019 | 0.006 | 2.895 | 12.031 / 12.070 |
| api.dispatch_reg0 | 1 | measured | 3 | 0.018 | 0.006 | 2.984 | 12.078 / 12.094 |
| api.dispatch_reg0 | 8 | measured | 3 | 0.018 | 0.006 | 3.211 | 10.918 / 12.062 |
| api.dispatch_reg_head | 1 | measured | 3 | 0.018 | 0.007 | 2.712 | 12.094 / 12.062 |
| api.dispatch_reg_head | 8 | measured | 3 | 0.018 | 0.006 | 2.976 | 12.090 / 12.090 |
| api.dispatch_reg_tail | 1 | measured | 3 | 0.019 | 0.006 | 3.062 | 12.066 / 12.078 |
| api.dispatch_reg_tail | 8 | measured | 3 | 0.019 | 0.006 | 3.404 | 12.062 / 12.070 |
| api.dispatch_regzero_word | 1 | measured | 3 | 0.019 | 0.006 | 2.995 | 12.062 / 12.078 |
| api.dispatch_regzero_word | 8 | measured | 3 | 0.019 | 0.006 | 2.960 | 12.059 / 12.066 |
| api.dispatch_shuffle_product | 1 | measured | 3 | 0.018 | 0.006 | 2.920 | 12.070 / 12.094 |
| api.dispatch_shuffle_product | 8 | measured | 3 | 0.019 | 0.006 | 2.912 | 12.066 / 12.090 |
| api.dispatch_shuffle_symbolic | 1 | measured | 3 | 0.019 | 0.006 | 3.024 | 12.062 / 12.090 |
| api.dispatch_shuffle_symbolic | 8 | measured | 3 | 0.018 | 0.006 | 2.973 | 12.090 / 12.062 |
| api.dispatch_simplify_with_vieta | 1 | measured | 3 | 0.018 | 0.006 | 2.856 | 12.066 / 12.078 |
| api.dispatch_simplify_with_vieta | 8 | measured | 3 | 0.018 | 0.007 | 2.729 | 12.059 / 12.059 |
| api.dispatch_sub | 1 | measured | 3 | 0.018 | 0.007 | 2.749 | 12.066 / 12.078 |
| api.dispatch_sub | 8 | measured | 3 | 0.018 | 0.006 | 3.066 | 12.051 / 12.051 |
| api.dispatch_sym_arith | 1 | measured | 3 | 0.031 | 0.007 | 4.499 | 15.012 / 12.094 |
| api.dispatch_sym_arith | 8 | measured | 3 | 0.030 | 0.007 | 4.519 | 15.027 / 12.090 |
| api.dispatch_zero_inf_period | 1 | measured | 3 | 0.027 | 0.008 | 3.388 | 15.012 / 12.059 |
| api.dispatch_zero_inf_period | 8 | measured | 3 | 0.027 | 0.008 | 3.586 | 15.020 / 12.066 |
| api.dispatch_zero_one_period | 1 | measured | 3 | 0.027 | 0.008 | 3.510 | 15.000 / 12.066 |
| api.dispatch_zero_one_period | 8 | measured | 3 | 0.027 | 0.007 | 3.635 | 15.027 / 12.047 |
| api.dispatch_break_up_contour | 1 | measured | 3 | 0.027 | 0.007 | 3.954 | 14.988 / 12.090 |
| api.dispatch_break_up_contour | 8 | measured | 3 | 0.027 | 0.007 | 4.092 | 15.004 / 12.047 |
| api.dispatch_convert_zero_one | 1 | measured | 3 | 0.018 | 0.007 | 2.676 | 12.070 / 12.094 |
| api.dispatch_convert_zero_one | 8 | measured | 3 | 0.019 | 0.006 | 3.210 | 12.062 / 12.078 |
| api.dispatch_evaluate_periods | 1 | measured | 3 | 0.028 | 0.008 | 3.501 | 15.008 / 12.062 |
| api.dispatch_evaluate_periods | 8 | measured | 3 | 0.027 | 0.007 | 3.757 | 15.047 / 12.051 |
| api.dispatch_factor | 1 | measured | 3 | 0.017 | 0.006 | 2.849 | 12.062 / 12.062 |
| api.dispatch_factor | 8 | measured | 3 | 0.018 | 0.006 | 3.062 | 12.078 / 12.094 |
| api.dispatch_factor_table | 1 | measured | 3 | 0.019 | 0.006 | 3.183 | 12.070 / 12.039 |
| api.dispatch_factor_table | 8 | measured | 3 | 0.017 | 0.006 | 2.993 | 12.090 / 12.066 |
| api.dispatch_fibration_basis | 1 | measured | 3 | 0.028 | 0.007 | 3.855 | 15.012 / 12.090 |
| api.dispatch_fibration_basis | 8 | measured | 3 | 0.026 | 0.007 | 3.831 | 15.027 / 12.090 |
| api.dispatch_pow | 1 | measured | 3 | 0.017 | 0.006 | 2.974 | 12.062 / 12.090 |
| api.dispatch_pow | 8 | measured | 3 | 0.017 | 0.006 | 3.055 | 12.070 / 12.066 |
| api.dispatch_rat_sub | 1 | measured | 3 | 0.018 | 0.006 | 2.854 | 12.059 / 12.094 |
| api.dispatch_rat_sub | 8 | measured | 3 | 0.017 | 0.006 | 3.132 | 12.090 / 12.090 |
| api.dispatch_rat_sum | 1 | measured | 3 | 0.018 | 0.006 | 3.085 | 12.090 / 12.055 |
| api.dispatch_rat_sum | 8 | measured | 3 | 0.017 | 0.006 | 3.025 | 12.090 / 12.066 |
| api.dispatch_reglim_word | 1 | measured | 3 | 0.026 | 0.006 | 4.205 | 15.012 / 12.094 |
| api.dispatch_reglim_word | 8 | measured | 3 | 0.026 | 0.006 | 4.006 | 14.992 / 12.066 |
| api.dispatch_series_expansion | 1 | measured | 3 | 0.017 | 0.006 | 3.000 | 12.094 / 12.094 |
| api.dispatch_series_expansion | 8 | measured | 3 | 0.016 | 0.005 | 2.988 | 12.062 / 12.062 |
| api.dispatch_sym_reduce | 1 | measured | 3 | 0.027 | 0.006 | 4.237 | 14.992 / 12.094 |
| api.dispatch_sym_reduce | 8 | measured | 3 | 0.027 | 0.006 | 4.330 | 14.992 / 12.051 |
| api.dispatch_test_zero_function | 1 | measured | 3 | 0.026 | 0.006 | 4.153 | 14.992 / 12.078 |
| api.dispatch_test_zero_function | 8 | measured | 3 | 0.025 | 0.006 | 4.295 | 15.012 / 12.078 |
| api.dispatch_transform_shuffle | 1 | measured | 3 | 0.026 | 0.006 | 4.002 | 14.980 / 12.094 |
| api.dispatch_transform_shuffle | 8 | measured | 3 | 0.026 | 0.006 | 4.234 | 15.012 / 12.066 |
| api.dispatch_transform_word | 1 | measured | 3 | 0.026 | 0.006 | 3.983 | 15.004 / 12.094 |
| api.dispatch_transform_word | 8 | measured | 3 | 0.026 | 0.006 | 4.192 | 15.008 / 12.066 |
| scale.multiply.d8.v1 | 1 | measured | 3 | 0.017 | 0.006 | 2.860 | 12.090 / 12.062 |
| scale.multiply.d8.v1 | 8 | measured | 3 | 0.017 | 0.006 | 3.107 | 12.078 / 12.094 |
| scale.multiply.d8.v3 | 1 | measured | 3 | 0.017 | 0.006 | 2.641 | 12.078 / 12.062 |
| scale.multiply.d8.v3 | 8 | measured | 3 | 0.016 | 0.006 | 2.936 | 12.070 / 12.051 |
| scale.multiply.d8.v8 | 1 | measured | 3 | 0.018 | 0.006 | 3.013 | 12.066 / 12.070 |
| scale.multiply.d8.v8 | 8 | measured | 3 | 0.017 | 0.006 | 3.004 | 12.066 / 12.094 |
| scale.multiply.d32.v1 | 1 | measured | 3 | 0.017 | 0.006 | 2.861 | 12.070 / 12.059 |
| scale.multiply.d32.v1 | 8 | measured | 3 | 0.018 | 0.006 | 3.247 | 12.066 / 12.062 |
| scale.multiply.d32.v3 | 1 | measured | 3 | 0.017 | 0.006 | 2.928 | 12.094 / 12.070 |
| scale.multiply.d32.v3 | 8 | measured | 3 | 0.017 | 0.006 | 2.883 | 12.090 / 12.094 |
| scale.multiply.d32.v8 | 1 | measured | 3 | 0.018 | 0.006 | 3.059 | 12.090 / 12.047 |
| scale.multiply.d32.v8 | 8 | measured | 3 | 0.017 | 0.006 | 2.985 | 12.078 / 12.062 |
| scale.multiply.d128.v1 | 1 | measured | 3 | 0.017 | 0.006 | 2.923 | 12.062 / 12.078 |
| scale.multiply.d128.v1 | 8 | measured | 3 | 0.018 | 0.006 | 3.158 | 12.062 / 12.078 |
| scale.multiply.d128.v3 | 1 | measured | 3 | 0.019 | 0.007 | 2.801 | 12.051 / 12.070 |
| scale.multiply.d128.v3 | 8 | measured | 3 | 0.018 | 0.006 | 3.078 | 12.070 / 12.090 |
| scale.multiply.d128.v8 | 1 | measured | 3 | 0.018 | 0.006 | 2.978 | 12.094 / 12.066 |
| scale.multiply.d128.v8 | 8 | measured | 3 | 0.018 | 0.006 | 2.965 | 12.094 / 12.078 |
| scale.dense_multiply.d4.v2 | 1 | measured | 3 | 0.017 | 0.006 | 2.718 | 12.078 / 12.062 |
| scale.dense_multiply.d4.v2 | 8 | measured | 3 | 0.017 | 0.006 | 2.981 | 12.062 / 12.070 |
| scale.dense_power.d4.v2 | 1 | measured | 3 | 0.018 | 0.006 | 2.901 | 12.090 / 12.070 |
| scale.dense_power.d4.v2 | 8 | measured | 3 | 0.018 | 0.006 | 3.124 | 12.066 / 12.066 |
| scale.dense_multiply.d4.v3 | 1 | measured | 3 | 0.017 | 0.006 | 2.828 | 12.094 / 12.090 |
| scale.dense_multiply.d4.v3 | 8 | measured | 3 | 0.017 | 0.006 | 2.845 | 12.078 / 12.055 |
| scale.dense_power.d4.v3 | 1 | measured | 3 | 0.017 | 0.006 | 3.005 | 12.090 / 12.059 |
| scale.dense_power.d4.v3 | 8 | measured | 3 | 0.017 | 0.006 | 3.039 | 10.910 / 12.066 |
| scale.dense_multiply.d8.v2 | 1 | measured | 3 | 0.017 | 0.006 | 2.819 | 12.090 / 12.047 |
| scale.dense_multiply.d8.v2 | 8 | measured | 3 | 0.014 | 0.006 | 2.514 | 12.090 / 12.094 |
| scale.dense_power.d8.v2 | 1 | measured | 3 | 0.017 | 0.006 | 2.911 | 12.066 / 12.090 |
| scale.dense_power.d8.v2 | 8 | measured | 3 | 0.012 | 0.004 | 2.775 | 12.066 / 12.090 |
| scale.dense_multiply.d8.v3 | 1 | measured | 3 | 0.019 | 0.008 | 2.475 | 12.066 / 12.094 |
| scale.dense_multiply.d8.v3 | 8 | measured | 3 | 0.013 | 0.005 | 2.536 | 12.078 / 12.074 |
| scale.dense_power.d8.v3 | 1 | measured | 3 | 0.014 | 0.005 | 2.817 | 12.070 / 12.090 |
| scale.dense_power.d8.v3 | 8 | measured | 3 | 0.013 | 0.005 | 2.786 | 12.078 / 12.094 |
| scale.dense_multiply.d12.v2 | 1 | measured | 3 | 0.013 | 0.004 | 2.859 | 12.070 / 12.051 |
| scale.dense_multiply.d12.v2 | 8 | measured | 3 | 0.012 | 0.004 | 2.838 | 12.066 / 12.047 |
| scale.dense_power.d12.v2 | 1 | measured | 3 | 0.012 | 0.005 | 2.699 | 12.070 / 12.055 |
| scale.dense_power.d12.v2 | 8 | measured | 3 | 0.012 | 0.004 | 2.941 | 12.066 / 12.094 |
| scale.dense_multiply.d12.v3 | 1 | measured | 3 | 0.017 | 0.007 | 2.287 | 15.582 / 15.488 |
| scale.dense_multiply.d12.v3 | 8 | measured | 3 | 0.017 | 0.007 | 2.472 | 15.602 / 15.477 |
| scale.dense_power.d12.v3 | 1 | measured | 3 | 0.017 | 0.006 | 2.675 | 15.594 / 12.059 |
| scale.dense_power.d12.v3 | 8 | measured | 3 | 0.017 | 0.006 | 2.754 | 15.602 / 15.484 |
| scale.parse_rational_power.n-3 | 1 | measured | 3 | 0.012 | 0.004 | 2.946 | 12.094 / 12.066 |
| scale.parse_rational_power.n-3 | 8 | measured | 3 | 0.013 | 0.004 | 3.093 | 12.066 / 12.059 |
| scale.parse_rational_power.n-2 | 1 | measured | 3 | 0.012 | 0.005 | 2.717 | 12.066 / 12.094 |
| scale.parse_rational_power.n-2 | 8 | measured | 3 | 0.014 | 0.005 | 2.908 | 12.066 / 12.066 |
| scale.parse_rational_power.n-1 | 1 | measured | 3 | 0.013 | 0.005 | 2.744 | 12.090 / 12.078 |
| scale.parse_rational_power.n-1 | 8 | measured | 3 | 0.013 | 0.005 | 2.865 | 12.062 / 12.094 |
| scale.parse_rational_power.n2 | 1 | measured | 3 | 0.013 | 0.004 | 2.993 | 12.094 / 12.094 |
| scale.parse_rational_power.n2 | 8 | measured | 3 | 0.016 | 0.005 | 3.409 | 12.051 / 12.070 |
| scale.parse_rational_power.n3 | 1 | measured | 3 | 0.013 | 0.004 | 3.074 | 12.059 / 12.090 |
| scale.parse_rational_power.n3 | 8 | measured | 3 | 0.013 | 0.004 | 2.974 | 12.055 / 12.094 |
| scale.parse_rational_power.n5 | 1 | measured | 3 | 0.013 | 0.004 | 2.912 | 12.090 / 12.062 |
| scale.parse_rational_power.n5 | 8 | measured | 3 | 0.012 | 0.005 | 2.737 | 12.094 / 12.070 |
| scale.rational_derivative.den1 | 1 | measured | 3 | 0.013 | 0.004 | 2.927 | 12.094 / 12.094 |
| scale.rational_derivative.den1 | 8 | measured | 3 | 0.013 | 0.004 | 2.858 | 12.070 / 12.094 |
| scale.rational_derivative.den2 | 1 | measured | 3 | 0.013 | 0.004 | 2.997 | 12.066 / 12.094 |
| scale.rational_derivative.den2 | 8 | measured | 3 | 0.014 | 0.004 | 3.158 | 12.066 / 12.090 |
| scale.rational_derivative.den4 | 1 | measured | 3 | 0.014 | 0.005 | 3.027 | 12.066 / 12.070 |
| scale.rational_derivative.den4 | 8 | measured | 3 | 0.017 | 0.006 | 3.019 | 12.062 / 12.090 |
| scale.rational_derivative.den8 | 1 | measured | 3 | 0.013 | 0.005 | 2.867 | 12.066 / 12.066 |
| scale.rational_derivative.den8 | 8 | measured | 3 | 0.018 | 0.004 | 4.184 | 12.090 / 12.066 |
| scale.gcd.d2 | 1 | measured | 3 | 0.013 | 0.005 | 2.861 | 12.078 / 12.070 |
| scale.gcd.d2 | 8 | measured | 3 | 0.015 | 0.004 | 3.476 | 12.066 / 12.094 |
| scale.gcd.d4 | 1 | measured | 3 | 0.014 | 0.005 | 2.893 | 12.094 / 12.051 |
| scale.gcd.d4 | 8 | measured | 3 | 0.018 | 0.005 | 3.968 | 12.094 / 12.055 |
| scale.gcd.d8 | 1 | measured | 3 | 0.014 | 0.005 | 2.958 | 12.090 / 12.066 |
| scale.gcd.d8 | 8 | measured | 3 | 0.014 | 0.004 | 3.054 | 12.078 / 12.062 |
| scale.resultant.d2 | 1 | measured | 3 | 0.013 | 0.004 | 2.856 | 12.070 / 12.062 |
| scale.resultant.d2 | 8 | measured | 3 | 0.012 | 0.004 | 3.002 | 12.059 / 12.059 |
| scale.resultant.d4 | 1 | measured | 3 | 0.013 | 0.004 | 2.891 | 12.059 / 12.090 |
| scale.resultant.d4 | 8 | measured | 3 | 0.012 | 0.004 | 2.980 | 12.078 / 12.066 |
| scale.resultant.d6 | 1 | measured | 3 | 0.013 | 0.005 | 2.840 | 12.062 / 12.090 |
| scale.resultant.d6 | 8 | measured | 3 | 0.015 | 0.005 | 3.232 | 12.090 / 12.094 |
| scale.partial_fractions.p2.m1 | 1 | measured | 3 | 0.015 | 0.005 | 2.891 | 13.902 / 12.066 |
| scale.partial_fractions.p2.m1 | 8 | measured | 3 | 0.019 | 0.006 | 3.154 | 13.914 / 12.094 |
| scale.partial_fractions.p3.m1 | 1 | measured | 3 | 0.020 | 0.005 | 3.706 | 13.930 / 12.078 |
| scale.partial_fractions.p3.m1 | 8 | measured | 3 | 0.020 | 0.007 | 3.043 | 13.906 / 12.066 |
| scale.partial_fractions.p4.m1 | 1 | measured | 3 | 0.020 | 0.007 | 2.757 | 13.902 / 12.090 |
| scale.partial_fractions.p4.m1 | 8 | measured | 3 | 0.020 | 0.007 | 2.796 | 13.926 / 12.070 |
| scale.partial_fractions.p2.m3 | 1 | measured | 3 | 0.020 | 0.005 | 3.896 | 13.918 / 12.094 |
| scale.partial_fractions.p2.m3 | 8 | measured | 3 | 0.018 | 0.007 | 2.738 | 13.934 / 10.473 |
| scale.partial_fractions.p3.m3 | 1 | measured | 3 | 0.020 | 0.008 | 2.530 | 12.090 / 12.066 |
| scale.partial_fractions.p3.m3 | 8 | measured | 3 | 0.020 | 0.007 | 2.900 | 13.953 / 12.059 |
| scale.partial_fractions.p4.m3 | 1 | measured | 3 | 0.024 | 0.010 | 2.409 | 13.914 / 12.090 |
| scale.partial_fractions.p4.m3 | 8 | measured | 3 | 0.024 | 0.010 | 2.523 | 13.980 / 12.078 |
| scale.partial_fractions.p2.m6 | 1 | measured | 3 | 0.020 | 0.007 | 2.615 | 13.926 / 12.090 |
| scale.partial_fractions.p2.m6 | 8 | measured | 3 | 0.020 | 0.007 | 2.865 | 13.973 / 12.066 |
| scale.partial_fractions.p3.m6 | 1 | measured | 3 | 0.025 | 0.013 | 1.934 | 13.949 / 14.559 |
| scale.partial_fractions.p3.m6 | 8 | measured | 3 | 0.026 | 0.012 | 2.213 | 13.926 / 14.570 |
| scale.partial_fractions.p4.m6 | 1 | measured | 3 | 0.087 | 0.045 | 1.950 | 23.102 / 26.582 |
| scale.partial_fractions.p4.m6 | 8 | measured | 3 | 0.088 | 0.044 | 2.000 | 26.074 / 26.531 |
| scale.laurent.order8 | 1 | measured | 3 | 0.020 | 0.007 | 3.026 | 12.066 / 12.066 |
| scale.laurent.order8 | 8 | measured | 3 | 0.019 | 0.006 | 3.054 | 12.066 / 12.090 |
| scale.laurent.order32 | 1 | measured | 3 | 0.021 | 0.007 | 3.133 | 12.070 / 12.066 |
| scale.laurent.order32 | 8 | measured | 3 | 0.019 | 0.006 | 3.185 | 12.078 / 12.078 |
| scale.laurent.order128 | 1 | measured | 3 | 0.037 | 0.008 | 4.529 | 12.066 / 12.070 |
| scale.laurent.order128 | 8 | measured | 3 | 0.035 | 0.007 | 4.739 | 12.070 / 12.090 |
| scale.period.zero_one.w2 | 1 | measured | 3 | 0.030 | 0.008 | 3.868 | 15.043 / 12.066 |
| scale.period.zero_one.w2 | 8 | measured | 3 | 0.023 | 0.007 | 3.425 | 15.059 / 12.055 |
| scale.period.zero_inf.w2 | 1 | measured | 3 | 0.030 | 0.008 | 3.766 | 15.012 / 12.094 |
| scale.period.zero_inf.w2 | 8 | measured | 3 | 0.030 | 0.008 | 3.909 | 15.020 / 12.090 |
| scale.period.zero_one.w4 | 1 | measured | 3 | 0.034 | 0.008 | 4.208 | 15.008 / 12.078 |
| scale.period.zero_one.w4 | 8 | measured | 3 | 0.034 | 0.008 | 4.490 | 15.008 / 12.059 |
| scale.period.zero_inf.w4 | 1 | measured | 3 | 0.041 | 0.009 | 4.591 | 15.004 / 12.078 |
| scale.period.zero_inf.w4 | 8 | measured | 3 | 0.040 | 0.008 | 4.762 | 15.027 / 12.070 |
| scale.period.zero_one.w6 | 1 | measured | 3 | 0.038 | 0.009 | 4.266 | 15.027 / 12.051 |
| scale.period.zero_one.w6 | 8 | measured | 3 | 0.038 | 0.009 | 4.340 | 15.008 / 12.062 |
| scale.period.zero_inf.w6 | 1 | measured | 3 | 0.083 | 0.012 | 6.945 | 15.023 / 12.066 |
| scale.period.zero_inf.w6 | 8 | measured | 3 | 0.086 | 0.013 | 6.841 | 15.008 / 12.078 |
| scale.period.zero_one.w8 | 1 | measured | 3 | 0.039 | 0.009 | 4.549 | 15.012 / 12.062 |
| scale.period.zero_one.w8 | 8 | measured | 3 | 0.039 | 0.009 | 4.427 | 15.031 / 12.094 |
| scale.period.zero_inf.w8 | 1 | measured | 3 | 0.303 | 0.031 | 9.745 | 15.023 / 12.090 |
| scale.period.zero_inf.w8 | 8 | measured | 3 | 0.305 | 0.031 | 9.712 | 15.039 / 12.094 |
| scale.integration.logw0 | 1 | measured | 3 | 0.028 | 0.008 | 3.583 | 18.051 / 12.066 |
| scale.integration.logw0 | 8 | measured | 3 | 0.028 | 0.009 | 3.078 | 18.082 / 14.523 |
| scale.integration.logw1 | 1 | measured | 3 | 0.030 | 0.008 | 3.554 | 18.039 / 12.066 |
| scale.integration.logw1 | 8 | measured | 3 | 0.029 | 0.009 | 3.139 | 18.027 / 14.508 |
| scale.integration.logw2 | 1 | measured | 3 | 0.030 | 0.009 | 3.452 | 18.078 / 12.066 |
| scale.integration.logw2 | 8 | measured | 3 | 0.030 | 0.010 | 3.161 | 18.035 / 14.566 |
| scale.integration.logw3 | 1 | measured | 3 | 0.029 | 0.008 | 3.483 | 18.027 / 12.090 |
| scale.integration.logw3 | 8 | measured | 3 | 0.029 | 0.009 | 3.004 | 18.062 / 14.535 |
| scale.integration.v1 | 1 | measured | 3 | 0.029 | 0.008 | 3.739 | 18.098 / 12.066 |
| scale.integration.v1 | 8 | measured | 3 | 0.028 | 0.009 | 3.007 | 18.047 / 14.531 |
| scale.integration.v2 | 1 | measured | 3 | 0.029 | 0.008 | 3.537 | 18.043 / 12.094 |
| scale.integration.v2 | 8 | measured | 3 | 0.029 | 0.009 | 3.091 | 18.051 / 14.438 |
| scale.integration.v3 | 1 | measured | 3 | 0.028 | 0.008 | 3.424 | 18.023 / 12.059 |
| scale.integration.v3 | 8 | measured | 3 | 0.029 | 0.010 | 2.839 | 18.074 / 14.547 |
| scale.integration.v5 | 1 | measured | 3 | 0.030 | 0.009 | 3.258 | 18.039 / 12.066 |
| scale.integration.v5 | 8 | measured | 3 | 0.029 | 0.010 | 2.975 | 18.055 / 14.539 |
| attachment.tst0 | 1 | measured | 3 | 0.786 | 0.230 | 3.420 | 18.023 / 22.512 |
| attachment.tst0 | 8 | measured | 3 | 0.398 | 0.101 | 3.934 | 51.035 / 38.668 |
| attachment.tst1 | 1 | measured | 3 | 13.630 | 6.120 | 2.227 | 36.594 / 42.504 |
| attachment.tst1 | 8 | measured | 3 | 1.870 | 1.250 | 1.496 | 91.453 / 142.836 |
| attachment.tst2 | 1 | measured | 3 | 202.797 | 80.797 | 2.510 | 401.898 / 106.023 |
| attachment.tst2 | 8 | measured | 3 | 28.456 | 27.319 | 1.042 | 478.234 / 210.859 |
| attachment.findroots21_a | 1 | measured | 3 | 0.180 | 0.018 | 9.791 | 30.234 / 14.508 |
| attachment.findroots21_a | 8 | measured | 3 | 0.224 | 0.020 | 11.157 | 30.184 / 14.590 |
| attachment.findroots21_b | 1 | measured | 3 | 0.060 | 0.011 | 5.411 | 30.234 / 14.480 |
| attachment.findroots21_b | 8 | measured | 3 | 0.062 | 0.012 | 5.045 | 30.207 / 14.633 |
| attachment.parity_face | 1 | measured | 3 | 42.533 | 25.741 | 1.652 | 59.117 / 164.746 |
| attachment.parity_face | 8 | measured | 3 | 6.252 | 6.547 | 0.955 | 107.809 / 353.133 |
| attachment.tst3 | 1 | measured | 3 | 2,072.049 | 1,569.987 | 1.320 | 7,522.613 / 1,243.594 |
| attachment.tst3 | 8 | measured | 3 | 302.991 | 345.647 | 0.877 | 7,991.531 / 1,355.320 |
| attachment.tst4 | 1 | measured | 3 | 37,886.765 | 23,457.092 | 1.615 | 135,209.496 / 18,466.910 |
| attachment.tst4 | 8 | measured | 3 | 4,101.630 | 4,287.707 | 0.957 | 140,278.031 / 18,778.641 |
| attachment.qbox_one_mass | 1 | failed | 0 | — | — | — | — / — |
| attachment.qbox_one_mass | 8 | failed | 0 | — | — | — | — / — |
| attachment.qbox_collaborator | 1 | failed | 0 | — | — | — | — / — |
| attachment.qbox_collaborator | 8 | failed | 0 | — | — | — | — / — |

## Graph inputs skipped by user request

- `attachment.dbox1m`: Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.
- `attachment.three_loop_form_factor`: Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.
- `attachment.tri_1L`: Literal-zero graph input; frontend unused-variable/zero handling must be specified. It is not a backend benchmark.
- `attachment.bub_1L`: Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.
- `attachment.sunset_2L`: Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.
- `attachment.ss2L_mid`: Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.
- `attachment.p3L_a`: Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.
- `attachment.vac3L`: Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.
- `attachment.vac4L`: Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.
- `attachment.vac5L`: Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.
- `attachment.box_1L`: Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.
- `attachment.pent1L`: Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.
- `attachment.lib10_equal_mass_bubble`: Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.
- `attachment.lib10_massive_triangle`: Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.
- `attachment.lib10_massive_box`: Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.
- `attachment.lib10_theta`: Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.
- `attachment.lib10_parachute`: Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.
- `attachment.lib10_bubble_pentagon`: Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.
- `attachment.lib10_sudakov`: Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.
- `attachment.tladder_L1`: Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.
- `attachment.tladder_L2`: Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.
- `attachment.tladder_L3`: Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.
- `attachment.tladder_L4`: Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.

[CSV](benchmark-evidence/full-suite-20260908.csv).

Raw outputs, individual samples, configuration, source hashes and
per-process CPU/memory/scheduler traces: `target/full-suite-20260908/`.
