#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_parens)]
#![allow(unused_mut)]

use std::fs;
use std::fs::File;
use std::io::Write;
use std::env;
use std::collections::HashSet;
use std::collections::HashMap;

// https://docs.rs/serde_json/latest/serde_json/
//use serde_json::json;
use serde::{Deserialize, Serialize};
use serde_json::Result;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SysCall {
    name: String,
    args: Vec<(String, String)>,
    returns: (String, String),
    permission: String,
    const_idx: Option<u16>,
    description: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SubSystem {
    subsystem: String,
    description: Option<String>,
    syscalls: Vec<SysCall>,
}

/// Verify that a string is a valid ascii identifier
fn is_valid_ident(name: &str) -> bool
{
    if name.len() == 0 {
        return false;
    }

    if name != name.to_lowercase() {
        return false;
    }

    let ch0 = name.chars().nth(0).unwrap();
    if ch0 != '_' && !ch0.is_ascii_alphabetic() {
        return false;
    }

    for ch in name.chars() {
        if ch != '_' && !ch.is_ascii_alphanumeric() {
            return false;
        }
    }

    return true;
}

fn main()
{
    let mut unique_names: HashSet<String> = HashSet::new();

    // Map from constant index to name
    let mut idx_to_name: Vec<Option<String>> = Vec::default();

    let syscalls_json = fs::read_to_string("syscalls.json").unwrap();
    let mut subsystems: Vec<SubSystem> = serde_json::from_str(&syscalls_json).unwrap();
    //println!("deserialized = {:?}", deserialized);

    // For each subsystem
    for subsystem in &subsystems {
        if !is_valid_ident(&subsystem.subsystem) {
            panic!();
        }

        // For each syscall for this subsystem
        for syscall in &subsystem.syscalls {
            if !is_valid_ident(&syscall.name) {
                panic!();
            }

            if syscall.args.len() % 2 != 0 {
                panic!();
            }

            if unique_names.get(&syscall.name).is_some() {
                panic!();
            }

            unique_names.insert(syscall.name.clone());

            // Fill the map of indices to names
            if let Some(const_idx) = syscall.const_idx {
                let const_idx = const_idx as usize;
                if const_idx >= idx_to_name.len() {
                    idx_to_name.resize(const_idx + 1, None);
                }

                if idx_to_name[const_idx].is_some() {
                    panic!();
                }

                idx_to_name[const_idx] = Some(syscall.name.clone());
            }
        }
    }

    // Verify that there are no gaps in the syscall indices
    for (idx, maybe_name) in idx_to_name.iter().enumerate() {
        if maybe_name.is_none() {
            panic!();
        }
    }

    // Allocate indices to the syscalls that have none
    for mut subsystem in &mut subsystems {
        for syscall in &mut subsystem.syscalls {
            if syscall.const_idx.is_none() {
                let const_idx = idx_to_name.len() as u16;
                syscall.const_idx = Some(const_idx);
                idx_to_name.push(Some(syscall.name.clone()));
                println!("allocating const_idx={} to syscall \"{}\"", const_idx, syscall.name);
            }

        }
    }

    // Re-serialize the data and write it back to the JSON file
    let json_output = serde_json::to_string_pretty(&subsystems).unwrap();
    //println!("{}", json_output);
    let mut file = File::create("syscalls.json").unwrap();
    file.write_all(json_output.as_bytes()).unwrap();


    // TODO: need a better name for the syscall constants
    //let mut file = File::create("syscalls.rs").unwrap();




    // TODO:
    // Generate syscall constants in rust







    // TODO:
    // Generate global array of syscall descriptors
    // Need to include name, const idx and arg count







}
