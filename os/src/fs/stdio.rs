use crate::fs::{StatMode};
use super::File;
use crate::mm::{UserBuffer};
use crate::sbi::console_getchar;
use crate::task::suspend_current_and_run_next;

/// The standard input
pub struct Stdin;
/// The standard output
pub struct Stdout;

impl File for Stdin {
    fn readable(&self) -> bool { true }
    fn writable(&self) -> bool { false }
    fn get_ino(&self) -> u32 { 0 }
    fn get_mode(&self) -> StatMode { StatMode::NULL }
    fn get_nlink(&self, target_block_id: u32, target_block_offset: usize) -> u32 { 0 }
    fn get_block_id(&self) -> u32 { 0 }
    fn get_block_offset(&self) -> usize { 0 }
    fn read(&self, mut user_buf: UserBuffer) -> usize {
        assert_eq!(user_buf.len(), 1);
        // busy loop
        let mut c: usize;
        loop {
            c = console_getchar();
            if c == 0 {
                suspend_current_and_run_next();
                continue;
            } else {
                break;
            }
        }
        let ch = c as u8;
        unsafe { user_buf.buffers[0].as_mut_ptr().write_volatile(ch); }
        1
    }
    fn write(&self, _user_buf: UserBuffer) -> usize {
        panic!("Cannot write to stdin!");
    }
}

impl File for Stdout {
    fn readable(&self) -> bool { false }
    fn writable(&self) -> bool { true }
    fn get_ino(&self) -> u32 { 0 }
    fn get_mode(&self) -> StatMode { StatMode::NULL }
    fn get_nlink(&self, target_block_id: u32, target_block_offset: usize) -> u32 { 0 }
    fn get_block_id(&self) -> u32 { 0 }
    fn get_block_offset(&self) -> usize { 0 }
    fn read(&self, _user_buf: UserBuffer) -> usize{
        panic!("Cannot read from stdout!");
    }
    fn write(&self, user_buf: UserBuffer) -> usize {
        for buffer in user_buf.buffers.iter() {
            print!("{}", core::str::from_utf8(*buffer).unwrap());
        }
        user_buf.len()
    }
}
