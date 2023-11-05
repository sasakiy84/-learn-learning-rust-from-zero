//! # regex crate
//! ## Example
//!
//! ```
//! use regex;
//! let expr = "a(bc)+|c(def)*";
//! let line = "cdefdefdef";
//! regex::do_matching(expr, line, true);
//! regex::print(expr);
//! ```
mod engine;
mod helper;

pub use engine::{do_matching, print};

