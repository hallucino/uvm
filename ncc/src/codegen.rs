use std::cmp::max;
use crate::ast::*;
use crate::parsing::{ParseError};
use crate::types::*;
use Type::*;

#[derive(Default)]
struct SymGen
{
    next_id: usize,
}

impl SymGen
{
    fn gen_sym(&mut self, prefix: &str) -> String
    {
        let name = format!("_{}_{}", prefix, self.next_id);
        self.next_id += 1;
        name
    }
}

// FIXME: ideally, all error checking should be done before we get to the
// codegen, so that codegen can't return an error?

impl Unit
{
    pub fn gen_code(&self) -> Result<String, ParseError>
    {
        let mut sym = SymGen::default();
        let mut out: String = "".to_string();

        out.push_str("#\n");
        out.push_str("# This file was automatically generated by the ncc compiler.\n");
        out.push_str("#\n");
        out.push_str("\n");

        out.push_str(".data;\n");
        out.push_str("\n");

        out.push_str("# Reserve the first heap word so we can use address 0 as null\n");
        out.push_str(".u64 0;\n");
        out.push_str("\n");

        out.push_str("__EVENT_LOOP_ENABLED__:\n");
        out.push_str(".u8 0;\n");
        out.push_str("\n");

        // Global variable initialization
        for global in &self.global_vars {
            // Align the data
            let align_bytes = global.var_type.align_bytes();
            out.push_str(&format!(".align {};\n", align_bytes));

            // Write a label
            out.push_str(&format!("{}:\n", global.name));

            match (&global.var_type, &global.init_expr) {
                (_, None) => {
                    out.push_str(&format!(".zero {};\n", global.var_type.sizeof()));
                }

                (Type::UInt(n), Some(Expr::Int(v))) => {
                    out.push_str(&format!(".u{} {};\n", n, v))
                }

                (Type::Int(n), Some(Expr::Int(v))) => {
                    out.push_str(&format!(".i{} {};\n", n, v))
                }

                (Type::Pointer(_), Some(Expr::Int(v))) => {
                    out.push_str(&format!(".u64 {};\n", v))
                }

                (Type::Pointer(_), Some(Expr::String(s))) => {
                    out.push_str(&format!(".stringz \"{}\";\n", s.escape_default()))
                }

                /*
                (Type::Array {..}, Expr::Int(0)) => {
                    out.push_str(&format!(".zero {};\n", global.var_type.sizeof()));
                }
                */

                _ => todo!()
            }

            out.push_str("\n");
        }

        out.push_str(&("#".repeat(78) + "\n"));
        out.push_str("\n");
        out.push_str(".code;\n");
        out.push_str("\n");

        // If there is a main function
        let main_fn: Vec<&Function> = self.fun_decls.iter().filter(|f| f.name == "main").collect();
        if let [main_fn] = main_fn[..] {
            //
            // TODO: support calling main with argc, argv as well
            //

            out.push_str("# call the main function and then exit\n");
            out.push_str("call main, 0;\n");
            out.push_str("push __EVENT_LOOP_ENABLED__;\n");
            out.push_str("load_u8;\n");
            out.push_str("jnz __ret_to_event_loop__;\n");
            out.push_str("exit;\n");
            out.push_str("__ret_to_event_loop__:\n");
            out.push_str("ret;\n");
            out.push_str("\n");
        }
        else
        {
            // If there is no main function, the unit should exit
            out.push_str("push 0;\n");
            out.push_str("exit;\n");
        }

        // Generate code for all the functions
        for fun in &self.fun_decls {
            fun.gen_code(&mut sym, &mut out)?;
        }

        Ok((out))
    }
}

impl Function
{
    fn needs_final_return(&self) -> bool
    {
        if let Stmt::Block(stmts) = &self.body {
            if stmts.len() > 0 {
                let last_stmt = &stmts[stmts.len() - 1];

                if let Stmt::ReturnVoid = last_stmt {
                    return false;
                }

                if let Stmt::ReturnExpr(_) = last_stmt {
                    return false;
                }
            }
        }

        return true;
    }

    fn gen_code(&self, sym: &mut SymGen, out: &mut String) -> Result<(), ParseError>
    {
        // Print the function signature in comments
        out.push_str(&format!("#\n"));
        out.push_str(&format!("# {} {}(", self.ret_type, self.name));
        for (idx, (p_type, p_name)) in self.params.iter().enumerate() {
            if idx > 0 {
                out.push_str(", ");
            }
            out.push_str(&format!("{} {}", p_type, p_name));
        }
        out.push_str(&format!(")\n"));
        out.push_str(&format!("#\n"));

        // Emit label for function
        out.push_str(&format!("{}:\n", self.name));

        // Allocate stack slots for the local variables
        for i in 0..self.num_locals {
            out.push_str("push 0;\n");
        }

        self.body.gen_code(&None, &None, sym, out)?;

        // If the body needs a final return
        if self.needs_final_return() {
            out.push_str("push 0;\n");
            out.push_str("ret;\n");
        }

        out.push_str("\n");

        Ok(())
    }
}

impl Stmt
{
    fn gen_code(
        &self,
        break_label: &Option<String>,
        cont_label: &Option<String>,
        sym: &mut SymGen,
        out: &mut String
    ) -> Result<(), ParseError>
    {
        match self {
            Stmt::Expr(expr) => {

                match expr {
                    // For assignment expressions as statements,
                    // avoid generating output that we would then need to pop
                    Expr::Binary { op: BinOp::Assign, lhs, rhs } => {
                        gen_assign(lhs, rhs, sym, out, false)?;
                    }

                    // For asm expressions with void output type, don't pop
                    // the output because no output is produced
                    Expr::Asm { out_type: Type::Void, .. } => {
                        expr.gen_code(sym, out)?;
                    }

                    _ => {
                        expr.gen_code(sym, out)?;
                        out.push_str("pop;\n");
                    }
                }
            }

            Stmt::Break => {
                match break_label {
                    Some(label) => out.push_str(&format!("jmp {};\n", label)),
                    None => return ParseError::msg_only("break outside of loop context")
                }
            }

            Stmt::Continue => {
                match cont_label {
                    Some(label) => out.push_str(&format!("jmp {};\n", label)),
                    None => return ParseError::msg_only("continue outside of loop context")
                }
            }

            // Return void
            Stmt::ReturnVoid => {
                out.push_str("push 0;\n");
                out.push_str("ret;\n");
            }

            Stmt::ReturnExpr(expr) => {
                if let Expr::Asm { out_type: Type::Void, .. } = expr.as_ref() {
                    expr.gen_code(sym, out)?;
                    out.push_str("push 0;\n");
                    out.push_str("ret;\n");
                }
                else
                {
                    expr.gen_code(sym, out)?;
                    out.push_str("ret;\n");
                }
            }

            Stmt::If { test_expr, then_stmt, else_stmt } => {
                test_expr.gen_code(sym, out)?;

                let false_label = sym.gen_sym("if_false");

                // If false, jump to else stmt
                out.push_str(&format!("jz {};\n", false_label));

                if else_stmt.is_some() {
                    let join_label = sym.gen_sym("if_join");

                    then_stmt.gen_code(break_label, cont_label, sym, out)?;
                    out.push_str(&format!("jmp {};\n", join_label));

                    out.push_str(&format!("{}:\n", false_label));
                    else_stmt.as_ref().unwrap().gen_code(break_label, cont_label, sym, out)?;
                    out.push_str(&format!("{}:\n", join_label));
                }
                else
                {
                    then_stmt.gen_code(break_label, cont_label, sym, out)?;
                    out.push_str(&format!("{}:\n", false_label));
                }
            }

            Stmt::While { test_expr, body_stmt } => {
                let loop_label = sym.gen_sym("while_loop");
                let break_label = sym.gen_sym("while_break");

                out.push_str(&format!("{}:\n", loop_label));
                test_expr.gen_code(sym, out)?;
                out.push_str(&format!("jz {};\n", break_label));

                body_stmt.gen_code(
                    &Some(break_label.clone()),
                    &Some(loop_label.clone()),
                    sym,
                    out
                )?;

                out.push_str(&format!("jmp {};\n", loop_label));
                out.push_str(&format!("{}:\n", break_label));
            }

            Stmt::DoWhile { test_expr, body_stmt } => {
                let loop_label = sym.gen_sym("dowhile_loop");
                let cont_label = sym.gen_sym("dowhile_cont");
                let break_label = sym.gen_sym("dowhile_break");

                out.push_str(&format!("{}:\n", loop_label));
                body_stmt.gen_code(
                    &Some(break_label.clone()),
                    &Some(cont_label.clone()),
                    sym,
                    out
                )?;

                out.push_str(&format!("{}:\n", cont_label));
                test_expr.gen_code(sym, out)?;
                out.push_str(&format!("jz {};\n", break_label));
                out.push_str(&format!("jmp {};\n", loop_label));

                out.push_str(&format!("{}:\n", break_label));
            }

            Stmt::For { init_stmt, test_expr, incr_expr, body_stmt } => {
                if init_stmt.is_some() {
                    init_stmt.as_ref().unwrap().gen_code(break_label, cont_label, sym, out)?;
                }

                let loop_label = sym.gen_sym("for_loop");
                let cont_label = sym.gen_sym("for_cont");
                let break_label = sym.gen_sym("for_break");

                out.push_str(&format!("{}:\n", loop_label));
                test_expr.gen_code(sym, out)?;
                out.push_str(&format!("jz {};\n", break_label));

                body_stmt.gen_code(
                    &Some(break_label.clone()),
                    &Some(cont_label.clone()),
                    sym,
                    out
                )?;

                out.push_str(&format!("{}:\n", cont_label));
                incr_expr.gen_code(sym, out)?;
                out.push_str("pop;\n");
                out.push_str(&format!("jmp {};\n", loop_label));

                out.push_str(&format!("{}:\n", break_label));
            }

            Stmt::Block(stmts) => {
                for stmt in stmts {
                    stmt.gen_code(break_label, cont_label, sym, out)?;
                }
            }

            _ => todo!()
        }

        Ok(())
    }
}

impl Expr
{
    fn gen_code(&self, sym: &mut SymGen, out: &mut String) -> Result<(), ParseError>
    {
        match self {
            Expr::Int(v) => {
                out.push_str(&format!("push {};\n", v));
            }

            Expr::Ref(decl) => {
                match decl {
                    Decl::Arg { idx, .. } => {
                        out.push_str(&format!("get_arg {};\n", idx));
                    }
                    Decl::Local { idx, .. } => {
                        out.push_str(&format!("get_local {};\n", idx));
                    }
                    Decl::Global { name, t } => {
                        out.push_str(&format!("push {};\n", name));
                        match t {
                            Type::UInt(n) => out.push_str(&format!("load_u{};\n", n)),
                            Type::Int(64) => out.push_str("load_u64;\n"),
                            Type::Int(32) => {
                                out.push_str("load_u32;\n");
                                out.push_str("sx_i32_i64;\n");
                            }
                            Type::Pointer(_) => {}
                            Type::Fun { .. } => {}
                            Type::Array { .. } => {}
                            _ => todo!()
                        }
                    }
                    Decl::Fun { name, t } => {
                        out.push_str(&format!("push {};\n", name));
                    }
                    //_ => todo!()
                }
            }

            Expr::Cast { new_type, child } => {
                use Type::*;

                let child_type = child.eval_type()?;
                child.gen_code(sym, out)?;

                match (&new_type, &child_type) {
                    // Cast to a larger type
                    (UInt(m), UInt(n)) => {},
                    (UInt(m), Int(n)) if m >= n => {},
                    (Int(m), UInt(n)) if m >= n => {},

                    (UInt(m), Int(n)) if m < n => {
                        out.push_str(&format!("trunc_u{};\n", m));
                    },

                    (Int(m), UInt(n)) if m < n => {
                        out.push_str(&format!("trunc_u{};\n", m));
                    },

                    // Pointer cast
                    (Pointer(_), Pointer(_)) => {},
                    (Pointer(_), Array{..}) => {},
                    (UInt(64), Pointer(_)) => {},
                    (Pointer(_), UInt(64)) => {},

                    _ => panic!("cannot cast to {} from {}", new_type, child_type)
                }
            }

            Expr::SizeofExpr { child } => {
                let t = child.eval_type()?;
                out.push_str(&format!("push {};\n", t.sizeof()));
            }

            Expr::SizeofType { t } => {
                out.push_str(&format!("push {};\n", t.sizeof()));
            }

            Expr::Unary { op, child } => {
                child.gen_code(sym, out)?;

                match op {
                    UnOp::Deref => {
                        let child_type = child.eval_type()?;

                        // If this is a pointer to an array, this is a noop
                        // because a pointer to an array is the array itself
                        if let Pointer(t) = child_type {
                            if let Array { .. } = t.as_ref() {
                                return Ok(())
                            }
                        }

                        let ptr_type = child.eval_type()?;
                        let elem_size = ptr_type.elem_type().sizeof();
                        let elem_bits = elem_size * 8;
                        out.push_str(&format!("load_u{};\n", elem_bits));
                    }

                    UnOp::Minus => {
                        out.push_str(&format!("push 0;\n"));
                        out.push_str(&format!("swap;\n"));
                        out.push_str(&format!("sub_u64;\n"));
                    }

                    UnOp::BitNot => {
                        let child_type = child.eval_type()?;
                        let num_bits = child_type.sizeof() * 8;
                        let op_bits = if num_bits <= 32 { 32 } else { 64 };
                        out.push_str(&format!("not_u{};\n", op_bits));

                        if num_bits < 32 {
                            out.push_str(&format!("trunc_u{};\n", num_bits));
                        }
                    }

                    // Logical negation
                    UnOp::Not => {
                        out.push_str("push 0;\n");
                        out.push_str("eq_u64;\n");
                    }

                    _ => todo!()
                }
            },

            Expr::Binary { op, lhs, rhs } => {
                let out_type = self.eval_type()?;
                gen_bin_op(op, lhs, rhs, &out_type, sym, out)?;
            }

            Expr::Ternary { test_expr, then_expr, else_expr } => {
                let false_label = sym.gen_sym("and_false");
                let done_label = sym.gen_sym("and_done");

                test_expr.gen_code(sym, out)?;
                out.push_str(&format!("jz {};\n", false_label));

                // Evaluate the then expression
                then_expr.gen_code(sym, out)?;
                out.push_str(&format!("jmp {};\n", done_label));

                // Evaluate the else expression
                out.push_str(&format!("{}:\n", false_label));
                else_expr.gen_code(sym, out)?;

                out.push_str(&format!("{}:\n", done_label));
            }

            Expr::Call { callee, args } => {
                //callee.gen_code(out)?;

                match callee.as_ref() {
                    Expr::Ref(Decl::Fun { name, .. }) =>
                    {
                        for arg in args {
                            arg.gen_code(sym, out)?;
                        }

                        out.push_str(&format!("call {}, {};\n", name, args.len()));
                    }
                    _ => todo!()
                }
            }

            Expr::Asm { text, args, out_type } => {
                for arg in args {
                    arg.gen_code(sym, out)?;
                }

                out.push_str(&text);
                out.push_str("\n");
            }

            _ => todo!("{:?}", self)
        }

        Ok(())
    }
}

/// Emit code for an integer operation
fn emit_int_op(out_type: &Type, signed_op: &str, unsigned_op: &str, out: &mut String)
{
    // Type checking should have caught invalid types before this point
    let out_bits = out_type.sizeof() * 8;
    assert!(out_bits <= 64);

    let op_bits = if out_bits == 64 { 64 } else { 32 };
    let op = if out_type.is_signed() { signed_op } else { unsigned_op };
    out.push_str(&format!("{}{};\n", op, op_bits));

    if out_bits < 32 {
        out.push_str(&format!("trunc_u{};\n", out_bits));
    }
}

/// Emit code for a comparison operation
fn emit_cmp_op(lhs_type: &Type, rhs_type: &Type, signed_op: &str, unsigned_op: &str, out: &mut String)
{
    let is_signed = lhs_type.is_signed() && rhs_type.is_signed();

    let num_bits = match (lhs_type, rhs_type) {
        (Int(m), UInt(n)) | (UInt(m), Int(n)) | (Int(m), Int(n)) | (UInt(m), UInt(n)) => *max(m, n),
        _ => 64
    };

    if num_bits <= 32 {
        if is_signed {
            out.push_str(&format!("{}32;\n", signed_op));
        } else {
            out.push_str(&format!("{}32;\n", unsigned_op));
        }
    } else {
        if is_signed {
            out.push_str(&format!("{}64;\n", signed_op));
        } else {
            out.push_str(&format!("{}64;\n", unsigned_op));
        }
    }
}

fn gen_bin_op(
    op: &BinOp,
    lhs: &Expr,
    rhs: &Expr,
    out_type: &Type,
    sym: &mut SymGen,
    out: &mut String
) -> Result<(), ParseError>
{
    use BinOp::*;
    use Type::*;

    // Assignments are different from other kinds of expressions
    // because we don't evaluate the lhs the same way
    if *op == Assign {
        gen_assign(lhs, rhs, sym, out, true)?;
        return Ok(());
    }

    // Comma sequencing operator: (a, b)
    if *op == Comma {
        lhs.gen_code(sym, out)?;
        out.push_str("pop;\n");
        rhs.gen_code(sym, out)?;
        return Ok(());
    }

    // Logical AND (a && b)
    if *op == And {
        let false_label = sym.gen_sym("and_false");
        let done_label = sym.gen_sym("and_done");

        // If a is false, the expression evaluates to false
        lhs.gen_code(sym, out)?;
        out.push_str(&format!("jz {};\n", false_label));

        // Evaluate the rhs
        rhs.gen_code(sym, out)?;
        out.push_str(&format!("jz {};\n", false_label));

        // Both subexpressions are true
        out.push_str("push 1;\n");
        out.push_str(&format!("jmp {};\n", done_label));

        out.push_str(&format!("{}:\n", false_label));
        out.push_str("push 0;\n");

        out.push_str(&format!("{}:\n", done_label));

        return Ok(());
    }

    // Logical OR (a || b)
    if *op == Or {
        let true_label = sym.gen_sym("or_true");
        let done_label = sym.gen_sym("or_done");

        // If a is true, the expression evaluates to true
        lhs.gen_code(sym, out)?;
        out.push_str(&format!("jnz {};\n", true_label));

        // Evaluate the rhs
        rhs.gen_code(sym, out)?;
        out.push_str(&format!("jnz {};\n", true_label));

        // Both subexpressions are false
        out.push_str("push 0;\n");
        out.push_str(&format!("jmp {};\n", done_label));

        out.push_str(&format!("{}:\n", true_label));
        out.push_str("push 1;\n");

        out.push_str(&format!("{}:\n", done_label));

        return Ok(());
    }

    lhs.gen_code(sym, out)?;
    rhs.gen_code(sym, out)?;

    let lhs_type = lhs.eval_type()?;
    let rhs_type = rhs.eval_type()?;
    let signed_op = lhs_type.is_signed() && rhs_type.is_signed();

    match op {
        BitAnd => {
            emit_int_op(out_type, "and_u", "and_u", out);
        }

        BitOr => {
            emit_int_op(out_type, "or_u", "or_u", out);
        }

        BitXor => {
            emit_int_op(out_type, "xor_u", "xor_u", out);
        }

        LShift => {
            emit_int_op(out_type, "lshift_u", "lshift_u", out);
        }

        RShift => {
            emit_int_op(out_type, "rshift_i", "rshift_u", out);
        }

        // For now we're ignoring the type
        Add => {
            match (lhs_type, rhs_type) {
                (Pointer(b), UInt(n)) | (Pointer(b), Int(n)) => {
                    let elem_sizeof = b.sizeof();
                    out.push_str(&format!("push {};\n", elem_sizeof));
                    out.push_str("mul_u64;\n");
                    out.push_str("add_u64;\n");
                }

                (Array{ elem_type , ..}, UInt(n)) | (Array{ elem_type , ..}, Int(n)) => {
                    let elem_sizeof = elem_type.sizeof();
                    out.push_str(&format!("push {};\n", elem_sizeof));
                    out.push_str("mul_u64;\n");
                    out.push_str("add_u64;\n");
                }

                (Int(m), UInt(n)) | (UInt(m), Int(n)) | (Int(m), Int(n)) | (UInt(m), UInt(n)) => {
                    emit_int_op(out_type, "add_u", "add_u", out);
                }

                _ => todo!()
            }
        }

        Sub => {
            match (&lhs_type, &rhs_type) {
                (Pointer(b), UInt(n)) | (Pointer(b), Int(n)) => {
                    let elem_sizeof = b.sizeof();
                    out.push_str(&format!("push {};\n", elem_sizeof));
                    out.push_str("mul_u64;\n");
                    out.push_str("sub_u64;\n");
                }

                (Int(m), UInt(n)) | (UInt(m), Int(n)) | (Int(m), Int(n)) | (UInt(m), UInt(n)) => {
                    emit_int_op(out_type, "sub_u", "sub_u", out);
                }

                _ => todo!("{:?} - {:?}", lhs, rhs)
            }
        }

        Mul => {
            out.push_str("mul_u64;\n");
        }

        Div => {
            match signed_op {
                true => out.push_str("div_i64;\n"),
                false => out.push_str("div_u64;\n"),
            }
        }

        Mod => {
            match signed_op {
                true => out.push_str("mod_i64;\n"),
                false => out.push_str("mod_u64;\n"),
            }
        }

        Eq => {
            emit_cmp_op(&lhs_type, &rhs_type, "eq_u", "eq_u", out);
        }

        Ne => {
            emit_cmp_op(&lhs_type, &rhs_type, "ne_u", "ne_u", out);
        }

        Lt => {
            emit_cmp_op(&lhs_type, &rhs_type, "lt_i", "lt_u", out);
        }

        Le => {
            emit_cmp_op(&lhs_type, &rhs_type, "le_i", "le_u", out);
        }

        Gt => {
            emit_cmp_op(&lhs_type, &rhs_type, "gt_i", "gt_u", out);
        }

        Ge => {
            emit_cmp_op(&lhs_type, &rhs_type, "ge_i", "ge_u", out);
        }

        _ => todo!("{:?}", op),
    }

    Ok(())
}

fn gen_assign(
    lhs: &Expr,
    rhs: &Expr,
    sym: &mut SymGen,
    out: &mut String,
    need_value: bool,
) -> Result<(), ParseError>
{
    //dbg!(lhs);
    //dbg!(rhs);

    match lhs {
        Expr::Unary { op, child } => {
            match op {
                UnOp::Deref => {
                    let ptr_type = child.eval_type()?;
                    let elem_size = ptr_type.elem_type().sizeof();
                    let elem_bits = elem_size * 8;

                    // If the output value is needed
                    if need_value {
                        // Evaluate the value expression
                        rhs.gen_code(sym, out)?;

                        // Evaluate the address expression
                        child.gen_code(sym, out)?;

                        out.push_str("getn 1;\n");
                    }
                    else
                    {
                        // Evaluate the address expression
                        child.gen_code(sym, out)?;

                        // Evaluate the value expression
                        rhs.gen_code(sym, out)?;
                    }

                    // store (addr) (value)
                    out.push_str(&format!("store_u{};\n", elem_bits));
                }
                _ => todo!()
            }
        },

        Expr::Ref(decl) => {
            match decl {
                Decl::Arg { idx, .. } => {
                    rhs.gen_code(sym, out)?;
                    if need_value { out.push_str("dup;\n"); }
                    out.push_str(&format!("set_arg {};\n", idx));
                }
                Decl::Local { idx, .. } => {
                    rhs.gen_code(sym, out)?;
                    if need_value { out.push_str("dup;\n"); }
                    out.push_str(&format!("set_local {};\n", idx));
                }

                Decl::Global { name, t } => {
                    // If the output value is needed
                    if need_value {
                        // Evaluate the value expression
                        rhs.gen_code(sym, out)?;

                        // Push the address
                        out.push_str(&format!("push {};\n", name));

                        out.push_str("getn 1;\n");
                    }
                    else
                    {
                        // Push the address
                        out.push_str(&format!("push {};\n", name));

                        // Evaluate the value expression
                        rhs.gen_code(sym, out)?;
                    }

                    match t {
                        Type::UInt(n) | Type::Int(n) => out.push_str(&format!("store_u{};\n", n)),
                        Type::Pointer(_) => out.push_str(&format!("store_u64;\n")),

                        _ => todo!()
                    }
                }

                _ => todo!()
            }
        }
        _ => todo!()
    }

    Ok(())
}

#[cfg(test)]
mod tests
{
    use super::*;

    fn gen_ok(src: &str) -> String
    {
        use crate::parsing::Input;
        use crate::parser::parse_unit;

        dbg!(src);
        let mut input = Input::new(&src, "src");
        let mut unit = parse_unit(&mut input).unwrap();
        unit.resolve_syms().unwrap();
        unit.check_types().unwrap();
        dbg!(&unit.fun_decls[0]);
        unit.gen_code().unwrap()
    }

    fn compile_file(file_name: &str)
    {
        use crate::parsing::Input;
        use crate::parser::parse_unit;
        use crate::cpp::process_input;

        dbg!(file_name);
        let mut input = Input::from_file(file_name);
        let output = process_input(&mut input).unwrap();
        //println!("{}", output);

        let mut input = Input::new(&output, file_name);
        let mut unit = parse_unit(&mut input).unwrap();
        unit.resolve_syms().unwrap();
        unit.check_types().unwrap();
        unit.gen_code().unwrap();
    }

    #[test]
    fn basics()
    {
        gen_ok("void main() {}");
        gen_ok("void foo(u64 a) {}");
        gen_ok("u64 foo(u64 a) { return a; }");
        gen_ok("u64 foo(u64 a) { return a + 1; }");
        gen_ok("u64 foo(u64 a) { return a; }");
        gen_ok("bool foo(u64 a, u64 b) { return a < b; }");

        // Local variables
        gen_ok("void main() { u64 a = 0; }");
        gen_ok("void main(u64 a) { u64 a = 0; }");
        gen_ok("void main(u64 a) { u64 b = a + 1; }");
        gen_ok("void main() { int a = 1; }");
        gen_ok("void main() { int c; c = 1; }");

        // Infix expressions
        gen_ok("u64 foo(u64 a, u64 b) { return a + b * 2; }");
        gen_ok("u64 foo() { return 1 + 2, 3; }");
        gen_ok("u64 foo(u64 a, u64 b, u64 c) { return a? b:c; }");
        gen_ok("u64 foo(u64 a, u64 b, u64 c) { return 1 + a? b:c; }");
        gen_ok("u64 foo(u64 a, u64 b, u64 c) { return a? b+1:c+2; }");
        gen_ok("bool foo(int a, int b) { return a < b; }");
        gen_ok("int foo(int a) { return a + 1; }");

        // Check that a return instruction is automatically inserted
        gen_ok("void foo() {}").contains("ret;");
    }

    #[test]
    fn globals()
    {
        gen_ok("unsigned char g = 255; void main() {}");
        gen_ok("int g = 5; void main() {}");
        gen_ok("u32 g = 5; void main() {}");
        gen_ok("u64 g = 5; u64 main() { return 0; }");
        gen_ok("u64 g = 5; u64 main() { return g; }");
        gen_ok("u64 g = 5; u64 main() { return g + 1; }");
        gen_ok("u8* p = null; u8* foo() { return p; }");
        gen_ok("u64 g = 0; void foo(u32 v) { g = v; }");
        gen_ok("i64 g = -77; i64 foo() { return g; }");
        gen_ok("i64 g = -77; void foo() { g = 1; }");
        gen_ok("bool levar = true; bool foo() { return levar; }");
        gen_ok("int g = 5; int f() { return g; }");
    }

    #[test]
    fn call_ret()
    {
        gen_ok("void foo() {} void bar() {}");
        gen_ok("void foo() {} void bar() { return foo(); } ");
        gen_ok("void print_i64(i64 v) {} void bar(u64 v) { print_i64(v); }");
    }

    #[test]
    fn pointers()
    {
        // Void pointers
        gen_ok("void foo(void* a) {}");
        gen_ok("void foo() { void* a = 0; }");
        gen_ok("void foo() { u64* a = 0; }");

        // Assignment to a pointer
        gen_ok("void foo(u64* a) { *a = 0; }");
        gen_ok("void foo(u64* a) { *(a + 1) = 0; }");
        gen_ok("void foo(u8* a) { *a = 0; }");
        gen_ok("void foo(u8* a) { *a = 255; }");
        gen_ok("void foo(u8* a) { *(a + 1) = 5; }");

        // Dereferencing a pointer
        gen_ok("u64 foo(u64* a) { return *a; }");
        gen_ok("u64 foo(u64* a) { return *(a + 1); }");
        gen_ok("u8 foo(u8* p) { return *(p + 1); }");

        // Assignment to a global pointer
        gen_ok("char* s; void f() { s = \"str\"; }");

        gen_ok("size_t strlen(char* p) { size_t l = 0; while (*(p + l) != 0) l = l + 1; return l; }");
    }

    #[test]
    fn strings()
    {
        gen_ok("char* str = \"foo\\nbar\"; void foo() {}");
        gen_ok("char* str = \"foo\\nbar\"; void bar(char* str) {} void foo() { bar(str); }");
        gen_ok("void bar(char* str) {} void foo() { bar(\"string constant\"); }");
    }

    #[test]
    fn arrays()
    {
        gen_ok("u8 PIXELS[800][600][3]; void foo() {}");
        gen_ok("u32 ARR[4]; void foo() { ARR[0] = 1; }");
    }

    #[test]
    fn if_else()
    {
        gen_ok("void foo(u64 a) { if (a) {} }");
        gen_ok("void foo(u64 a) { if (a) {} else {} }");
        gen_ok("void foo(u64 a, u64 b) { if (a || b) {} }");
        gen_ok("void foo(u64 a, u64 b) { if (a && b) {} }");
    }

    #[test]
    fn for_loop()
    {
        gen_ok("void foo(size_t n) { for (;;) {} }");
        gen_ok("void foo(size_t n) { for (size_t i = 0;;) {} }");
        gen_ok("void foo(size_t n) { for (size_t i = 0; i < n;) {} }");
        gen_ok("void foo(size_t n) { for (size_t i = 0; i < n; i = i + 1) {} }");
        gen_ok("void foo(size_t n) { for (size_t i = 0; i < n; i = i + 1) { break; } }");
        gen_ok("void foo(size_t n) { for (size_t i = 0; i < n; i = i + 1) { continue; } }");
        gen_ok("void foo(int n) { for (int i = 0; i < n; ++i) {} }");
    }

    #[test]
    fn compile_files()
    {
        // Make sure that we can compile all the examples
        for file in std::fs::read_dir("./examples").unwrap() {
            let file_path = file.unwrap().path().display().to_string();
            if file_path.ends_with(".c") {
                println!("{}", file_path);
                compile_file(&file_path);
            }
        }
    }
}
