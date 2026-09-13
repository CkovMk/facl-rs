/*
 * Copyright 2026 Chekhov Ma <maqike@qq.com>
 * SPDX-License-Identifier: Apache-2.0
 */
mod app;
mod backend;
mod cli;
mod event;
mod model;

use anyhow::Result;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    let _app = app::App::new(cli)?;
    Ok(())
}
