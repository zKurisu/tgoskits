#![allow(unused)]

use proc_macro::TokenStream;
use syn::parse;

fn ctx_store_x_q() -> String {
    let mut out = reg_op_pair("stp", "x", 1..32, "[sp,#-0x10]!", true);
    out = format!(
        "{out}\r\n    mrs	x9,     SPSR_EL1
    stp x9, x0, [sp,#-0x10]!"
    );
    out
}

fn reg_op_pair(
    op: &str,
    reg: &str,
    range: std::ops::Range<usize>,
    after: &str,
    reverse: bool,
) -> String {
    let mut ls = range
        .step_by(2)
        .map(|p0| {
            format!(
                "{op} {:>3},{:>3},    {after}",
                format!("{reg}{p0}"),
                format!("{reg}{}", p0 + 1)
            )
        })
        .collect::<Vec<_>>();

    if reverse {
        ls.reverse();
    }

    ls.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reg_op_pair() {
        let want = "
ldp  X1, X2,    [sp], #0x10
ldp  X3, X4,    [sp], #0x10
ldp  X5, X6,    [sp], #0x10
ldp  X7, X8,    [sp], #0x10
ldp  X9,X10,    [sp], #0x10
ldp X11,X12,    [sp], #0x10
ldp X13,X14,    [sp], #0x10
ldp X15,X16,    [sp], #0x10
ldp X17,X18,    [sp], #0x10
ldp X19,X20,    [sp], #0x10
ldp X21,X22,    [sp], #0x10
ldp X23,X24,    [sp], #0x10
ldp X25,X26,    [sp], #0x10
ldp X27,X28,    [sp], #0x10
ldp X29,X30,    [sp], #0x10
";

        let a_str = reg_op_pair("ldp", "X", 1..31, "[sp], #0x10", false);

        println!("{a_str}");

        assert_eq!(a_str.trim(), want.trim());
    }
}
