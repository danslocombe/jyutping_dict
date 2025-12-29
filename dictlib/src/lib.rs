#![allow(dead_code)]
#![allow(unused_parens)]
#![allow(static_mut_refs)]
#![allow(non_upper_case_globals)]

#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::len_zero)]
#![allow(clippy::identity_op)]
#![allow(clippy::needless_range_loop)]

#![feature(thread_local)]

use std::time::Instant;

use jyutping_splitter::JyutpingSplitter;
use serde::Serialize;

#[macro_export]
macro_rules! debug_log {
    ( $( $t:tt )* ) => {
        $crate::debug_logline(&format!( $( $t )* ));
    }
}

#[macro_export]
macro_rules! error_log {
    ( $( $t:tt )* ) => {
        $crate::error_logline(&format!( $( $t )* ));
    }
}

pub mod compiled_dictionary;
pub mod jyutping_splitter;
pub mod data_writer;
pub mod data_reader;
pub mod vbyte;
pub mod string_search;
pub mod rendered_result;
pub mod builder;
pub mod search;
pub mod reconstruct_match;

static mut DEBUG_LOGGER : Option<Box<dyn DebugLogger>> = None;

pub fn set_debug_logger(logger : Box<dyn DebugLogger>) {
    // Only should be called once by main thread at init
    unsafe {
        DEBUG_LOGGER = Some(logger);
    }
}

pub fn debug_logline(logline : &str)
{
    unsafe { if let Some(x) = DEBUG_LOGGER.as_ref() { x.log(logline); }}
}

pub fn error_logline(logline : &str)
{
    unsafe { if let Some(x) = DEBUG_LOGGER.as_ref() { x.log_error(logline); }}
}

pub trait DebugLogger {
    fn log(&self, logline: &str);
    fn log_error(&self, logline: &str);
}

pub trait Stopwatch {
    fn elapsed_ms(&self) -> i32;
}

pub struct NativeStopwatch {
    start : Instant,
}

impl NativeStopwatch {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Stopwatch for NativeStopwatch {
    fn elapsed_ms(&self) -> i32 {
        Instant::now().duration_since(self.start).as_millis() as i32
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OffsetString {
    pub start: u32,
    pub len: u32,
}

#[derive(Debug, Clone, Default)]
pub struct StringVecSet
{
    pub inner: Vec<String>,
}

impl StringVecSet {
    pub fn single(x: String) -> Self {
        Self {
            inner: vec![x],
        }
    }
    pub fn contains(&self, x: &str) -> bool {
        for xx in &self.inner {
            if (xx.eq_ignore_ascii_case(x)) {
                return true;
            }
        }

        false
    }

    pub fn add_clone(&mut self, val: &str) {
        if (!self.contains(val))
        {
            self.inner.push(val.to_owned());
        }
    }

    pub fn add(&mut self, val: String) {
        if (!self.contains(&val))
        {
            self.inner.push(val);
        }
    }

    // Similar to Vec::extend, drain other and add to our collection.
    pub fn extend(&mut self, other: StringVecSet) {
        for x in other.inner {
            self.add(x);
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub enum EntrySource {
    CEDict,
    CCanto,
}
