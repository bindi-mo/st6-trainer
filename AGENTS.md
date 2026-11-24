# Agent Guidelines & Operational Rules

This project is a desktop application built with **Tauri (Rust) + React (TypeScript) + Bun**.
To prevent tool invocation failures and unnecessary trial-and-error, agents must strictly adhere to the following rules.

---

## 0. Initial Platform Verification (Mandatory First Step)

* **Verify OS & Shell First**: Before running any terminal commands or modifying files, **always check the current OS and shell environment first** (e.g., check `$PSVersionTable` or `echo $OSTYPE` / `uname -a`).
* **Do Not Use `head` on Windows**: If the platform is **Windows (PowerShell)**, **never use the Unix `head` command**. Instead, use PowerShell's `Select-Object` (e.g., `Get-Content <file> | Select-Object -First 50`) or the dedicated `Read` tool with `view_range`.

---

## 1. Environment & Command Execution Rules (Bun / Cargo)

* **Shell Compatibility**: Adapt command syntax to the platform verified in Step 0. On Windows PowerShell where Unix commands (`head`, `which`, `grep`, etc.) are unavailable, use PowerShell equivalents (`Select-Object`, `Get-Command`, `Select-String`) or dedicated tools (`Read`, `Search`, `Find files`).
* **Package Manager / Runner**: This project uses **Bun**. Do not use `npm` or `npx`; always use `bun` / `bunx`.
  * Frontend type check: `bunx tsc --noEmit`
  * Frontend build: `bunx vite build`
  * Frontend tests: `bunx vitest run`
  * Rust check: `cd src-tauri && cargo check`
  * Rust tests: `cd src-tauri && cargo test`

---

## 2. File Operation Tool Usage

* **Strict Distinction Between New and Existing Files**:
  * Use the `Create File` tool **only** when creating a brand-new file.
  * When updating an **existing file** (e.g., `src/App.tsx`, `src/App.css`, `src-tauri/src/commands.rs`), never use `Create File`—even if rewriting the entire file. Always use the `Edit File` tool (writing files via PowerShell commands like `Out-File` is prohibited to prevent encoding and line-ending corruption).
* **Improving `Edit File` Accuracy**:
  * When specifying `old_str`, include **3–5 lines of surrounding context** so that the target block is uniquely identifiable within the file.
  * Never specify generic single lines such as `return (` or `}` as `old_str`.
  * Always inspect the latest lines of the target file before editing; do not construct `old_str` from memory or guesswork.

---

## 3. SQL / Task Management Tool Rules

* **One Statement Per Query Principle**:
  * When using the SQL execution tool (e.g., for `todos` table operations), do not chain multiple `INSERT` or `UPDATE` statements with semicolons (`;`) in a single call.
  * Execute only a **single SQL statement (single-line)** per tool invocation.

---

## 4. Coding & Type-Checking Workflow

* **Pre-Edit Type & Configuration Verification**:
  * Before writing or modifying TypeScript code, check `tsconfig.json` (such as `strict` or `noUnusedLocals` settings) as needed, and ensure no unused variables, imports, or functions are left behind.
  * If the API signature of an external library (e.g., `@tauri-apps/plugin-dialog`) is unclear, do not guess and iterate through build errors. Instead, inspect `node_modules/@tauri-apps/<package>/dist-js/*.d.ts` first to confirm the exact type definitions.
* **Efficient Verification Cycle**:
  * When creating or editing multiple components, do not run a build after every single file. Run `bunx tsc --noEmit` or `cargo check` together once a cohesive set of changes is complete.