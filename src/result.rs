// Result type that can contain any type of Error via boxing.
//
// Copyright 2021 Robbert Haarman
//
// SPDX-License-Identifier: MIT

pub type BoxResult<T> = Result<T, Box<dyn std::error::Error>>;
