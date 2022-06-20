#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_mut)]
#![allow(unused_imports)]

mod vm;
mod asm;
mod display;

use std::env;
use crate::vm::{VM, MemBlock, Op};
use crate::asm::{Assembler};

fn main()
{
    let args: Vec<String> = env::args().collect();
    println!("{:?}", args);

    if args.len() == 2 {
        let asm = Assembler::new();
        let code = asm.parse_file(&args[1]);

        let mut vm = VM::new(code);
        vm.eval();

        if vm.stack_size() > 0
        {
            let ret = vm.pop();
            println!("ret: {:?}", ret);
        }
        else
        {
            println!("vm stack empty");
        }

        return;
    }







}
