use jinux_frame::cpu::CpuContext;

use super::{constants::*, SyscallReturn};
use crate::log_syscall_entry;
use crate::process::elf::load_elf_to_root_vmar;
use crate::util::{read_cstring_from_user, read_val_from_user};
use crate::{prelude::*, syscall::SYS_EXECVE};

pub fn sys_execve(
    filename_ptr: Vaddr,
    argv_ptr_ptr: Vaddr,
    envp_ptr_ptr: Vaddr,
    context: &mut CpuContext,
) -> Result<SyscallReturn> {
    log_syscall_entry!(SYS_EXECVE);
    let filename = read_cstring_from_user(filename_ptr, MAX_FILENAME_LEN)?;
    let argv = read_cstring_vec(argv_ptr_ptr, MAX_ARGV_NUMBER, MAX_ARG_LEN)?;
    let envp = read_cstring_vec(envp_ptr_ptr, MAX_ENVP_NUMBER, MAX_ENV_LEN)?;
    debug!(
        "filename: {:?}, argv = {:?}, envp = {:?}",
        filename, argv, envp
    );
    if filename != CString::new("./hello").unwrap() {
        panic!("Unknown filename.");
    }

    let elf_file_content = crate::user_apps::read_execve_hello_content();
    let current = current!();
    // destroy root vmars
    let root_vmar = current
        .root_vmar()
        .expect("[Internal Error] User process should have vm space");
    root_vmar.clear()?;
    let user_vm = current
        .user_vm()
        .expect("[Internal Error] User process should have user vm");
    user_vm.set_default();
    // load elf content to new vm space
    let elf_load_info = load_elf_to_root_vmar(filename, elf_file_content, root_vmar, argv, envp)
        .expect("load elf failed");
    debug!("load elf in execve succeeds");
    // set signal disposition to default
    current.sig_dispositions().lock().inherit();
    // set cpu context to default
    let defalut_content = CpuContext::default();
    context.gp_regs = defalut_content.gp_regs;
    context.fs_base = defalut_content.fs_base;
    context.fp_regs = defalut_content.fp_regs;
    // set new entry point
    context.gp_regs.rip = elf_load_info.entry_point();
    debug!("entry_point: 0x{:x}", elf_load_info.entry_point());
    // set new user stack top
    context.gp_regs.rsp = elf_load_info.user_stack_top();
    debug!("user stack top: 0x{:x}", elf_load_info.user_stack_top());
    Ok(SyscallReturn::NoReturn)
}

fn read_cstring_vec(
    array_ptr: Vaddr,
    max_string_number: usize,
    max_string_len: usize,
) -> Result<Vec<CString>> {
    let mut res = Vec::new();
    let mut read_addr = array_ptr;
    let mut find_null = false;
    for _ in 0..max_string_number {
        let cstring_ptr = read_val_from_user::<usize>(read_addr)?;
        read_addr += 8;
        // read a null pointer
        if cstring_ptr == 0 {
            find_null = true;
            break;
        }
        let cstring = read_cstring_from_user(cstring_ptr, max_string_len)?;
        res.push(cstring);
    }
    if !find_null {
        return_errno_with_message!(Errno::E2BIG, "Cannot find null pointer in vector");
    }
    Ok(res)
}