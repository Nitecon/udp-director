[← Back to README](../README.md)

# Rust Coding Guidelines

- **ID:** `RUST-GUIDE-001`
- **Status:** `Active`
- **Version:** `1.0.0`

## 1. Overview

These guidelines are the single source of truth for all Rust code in this repository. They are based on community best practices, prioritizing consistency, simplicity, and maintainability. All contributions must adhere to these rules.

## 2. Core Principles

1.  **Consistency:** Code should look as if written by a single person.
2.  **Simplicity (KIS):** Prefer clear, idiomatic Rust over "clever" solutions.
3.  **Enforceability:** Guidelines are enforced by automated tooling.

## 3. Tooling & Environment

### 3.1. Toolchain

-   **Rustup:** All developers must use `rustup` to manage their toolchains.
-   **Edition:** This project targets the **Rust 2024 Edition**.
-   **Channels:** Use the `stable` channel.

### 3.2. Formatting

Formatting is non-negotiable and is enforced by CI.
-   **Tool:** `rustfmt`
-   **Configuration:** We use the **default `rustfmt` settings**. No `rustfmt.toml` file is required.
-   **Command:** Run `cargo fmt` before every commit.

### 3.3. Linting

Linting helps catch common bugs and un-idiomatic code.
-   **Tool:** `clippy`
-   **Configuration:** We use a strict set of lints based on `clippy::pedantic`. Exceptions are managed in the root `clippy.toml` file.
-   **Command:** Run `cargo clippy -- -D warnings` before opening a pull request. CI will enforce this.

Ensure that if it doesn't exist that you create a `clippy.toml` in the root of the repository with the following content that can be adjusted as needed:
```toml
# This file configures 'clippy', Rust's linter.
# The CI command `cargo clippy -- -D warnings -D clippy::pedantic`
# enables all warnings and pedantic lints as errors.
#
# This file lists the lints we explicitly 'allow' (disable)
# from that very strict set.

[lints]

# 'pedantic' lints we've agreed to allow:

# Allow implicit returns (e.g., `x + 1` instead of `return x + 1;`)
# This is highly idiomatic Rust.
clippy::implicit_return = "allow"

# Allows structs/enums that are small in bytes. Pedantic suggests boxing.
clippy::large_enum_variant = "allow"

# Allows modules with names that partially match parent module names.
clippy::module_name_repetitions = "allow"

# Allows `if/else` with a single `if`. Pedantic wants `if` only.
clippy::single_match_else = "allow"

# 'restriction' lints (often part of pedantic) we allow:

# Allow indexing (`my_vec[0]`). Pedantic prefers `.get()`.
# This is fine in code where logic guarantees bounds, and avoids
# unnecessary `.unwrap()` or error handling.
clippy::indexing_slicing = "allow"

# Allow integer/float arithmetic that *could* panic/wrap.
# This is often too noisy for performance-sensitive code.
clippy::arithmetic_side_effects = "allow"

# Allow non-obvious casts (e.g., `u64` as `usize`).
clippy::cast_possible_truncation = "allow"
clippy::cast_possible_wrap = "allow"
clippy::cast_sign_loss = "allow"
```

## 4. Formatting & Naming Conventions

### 4.1. Formatting

`cargo fmt` handles all formatting. Do not manually format code.

### 4.2. Naming

We follow standard Rust API guidelines:
-   **Types & Traits:** `PascalCase` (e.g., `MyStruct`, `MyTrait`).
-   **Functions & Variables:** `snake_case` (e.g., `my_function`, `my_variable`).
-   **Constants & Statics:** `SCREAMING_SNAKE_CASE` (e.g., `MAX_RETRIES`).
-   **Modules:** `snake_case` (e.g., `my_module.rs`).

## 5. Code Organization & Project Structure

### 5.1. Modules

-   **Roots:** `src/lib.rs` (for library crates) and `src/main.rs` (for binary crates) are the crate roots.
-   **Binaries:** Additional binaries are placed in `src/bin/my_binary.rs`.
-   **Module Files:** A module `my_module` declared in `lib.rs` (`mod my_module;`) should live in `src/my_module.rs`.
-   **Module Folders:** Only use `src/my_module/mod.rs` if the module is complex and contains sub-modules (e.g., `src/my_module/utils.rs`). Prefer the flat `src/my_module.rs` file structure.

### 5.2. Visibility

-   **`pub`:** Use for items that are part of the *public API* of a library.
-   **`pub(crate)`:** Use for items that need to be accessed from other modules *within the same crate* but are not part of the public API. This is the preferred default for internal crate-wide helpers.
-   **Private (default):** Use for items that are only used within their own module.

## 6. Idiomatic Rust & Best Practices

### 6.1. Error Handling

This is the most critical part of robust Rust code.
-   **Use `Result<T, E>`:** All fallible functions *must* return a `Result`.
-   **No `.unwrap()` or `.expect()`:** These calls are **strictly forbidden** in all production code paths. They are permitted *only* in tests, examples, or at the very top of `main.rs` if a failure is unrecoverable on startup.
-   **Library Error Types:** When writing library code (e.g., in `src/lib.rs` or its modules), define custom error enums using the `thiserror` crate. This provides structured, meaningful errors to consumers.
-   **Application Error Handling:** In application logic (e.g., `src/main.rs`, `src/bin/`), use the `anyhow` crate to easily propagate and add context to errors. `anyhow::Result<T>` is a drop-in replacement for `Result<T, E>` that simplifies handling diverse error types.

### 6.2. Pattern Matching

-   Prefer `match`, `if let`, and `while let` over complex, nested `if/else` statements for checking `enum` variants or `Option` values.

### 6.3. Ownership & Borrowing

-   **Prefer Borrows:** Always pass a borrow (`&T` or `&mut T`) to a function unless the function *must* take ownership of the value.
    -   *Analogy:* Think of this as the compiler enforcing you to pass a pointer (a borrow) instead of making a copy (a `clone`), unless you explicitly ask for the copy.
-   **Avoid `.clone()`:** A `.clone()` indicates a new heap allocation and copy. While necessary at times (especially for `Arc`), treat it as a "red flag" to review. Can you use a borrow instead?

### 6.4. Collections

-   **`Vec<T>`:** The default dynamic array. (Like Go's `slice` or C++'s `std::vector`).
-   **`HashMap<K, V>`:** The default hash map. (Like Go's `map` or C++'s `std::unordered_map`).
-   **`&str` vs. `String`:**
    -   Use `&str` (a string slice) for function parameters that only need to *read* string data. It's the equivalent of Go's `string` or C++'s `std::string_view`.
    -   Use `String` (an owned, heap-allocated string) for struct fields or functions that need to *build or own* string data. It's the equivalent of C++'s `std::string` or what a Go `strings.Builder` produces.

## 7. Unsafe Code

-   **Forbidden by Default:** The use of `unsafe` blocks is forbidden.
-   **Justification Required:** Any exception *must* be approved by the project owner and accompanied by a mandatory `// SAFETY:` comment block.
    ```rust
    // SAFETY:
    // 1. Explain why this code is necessary (e.g., FFI).
    // 2. Document the invariants that *must* be upheld for this
    //    block to be sound.
    unsafe {
        // ... unsafe operations ...
    }
    ```

## 8. Concurrency

-   **`async/await`:** Use for I/O-bound concurrency (e.g., network requests, file I/O) with a runtime like `tokio`.
-   **`std::thread`:** Use for CPU-bound tasks (e.g., parallel computation).
-   **Shared State:**
    -   **`Arc<T>`:** (Atomic Reference Counted) Use to share *immutable* ownership of a value across multiple threads. (Like C++'s `std::shared_ptr`).
    -   **`Mutex<T>` / `RwLock<T>`:** Use for *interior mutability* of shared state.
    -   *Analogy:* Unlike Go's `sync.Mutex` which is separate from the data, Rust's `Mutex<T>` *owns* the data `T`. You cannot access the data without first acquiring the lock, which the compiler enforces.


## 9. Dependencies

-   **Minimization:** Keep the dependency tree as small as possible.
-   **Vetting:** Before adding a new crate, vet it for maintainability, security, and necessity. Run `cargo audit` regularly.
-   **Updates:** Keep dependencies updated using `cargo update`.

## 10. Testing

-   **Unit Tests:** New logic *must* be accompanied by unit tests. Place these in a `#[cfg(test)] mod tests { ... }` block at the bottom of the file they are testing.
-   **Integration Tests:** Place tests that use the *public API* of your library in the `tests/` directory at the repository root. Each file is compiled as a separate crate.

## 11. Documentation

Documentation is mandatory, not optional.

### 11.1. Code Documentation (rustdoc)

-   **Public API:** All `pub` items (functions, structs, traits, enums, modules) must have `rustdoc` comments (`///`).
    -   Explain what the item does.
    -   Explain why it exists (its purpose).
    -   Explain any non-obvious parameters or return values.
    -   Provide at least one `/// # Examples` block.
-   **Internal Comments:** Non-obvious internal logic (`//`) must be commented to explain the why, not the what.

### 11.2. Repository Documentation (Docs directory)

-   **Primary location:** The `Docs/` directory is the single source of truth for repository documentation intended for users and contributors.
-   **Format:** All documentation in `Docs/` must be written in Markdown (`.md`).
-   **Backlinks:** Every Markdown file in `Docs/` must include a backlink to the root `README.md` at the top and at the bottom of the file, using a relative link: `[← Back to README](../README.md)`.
-   **Root README as ToC:** The root `README.md` must provide:
    -   A concise, high-level overview of the project (what it is, why it exists, key features).
    -   A "Documentation" section that serves as a table of contents linking to the Markdown files under `Docs/`.
-   **Keeping links up-to-date:** When adding, renaming, or removing files in `Docs/`, update the root `README.md` table of contents in the same change.
-   **Suggested organization:**
    -   `Docs/Architecture.md` — system and component architecture.
    -   `Docs/Setup.md` — installation, configuration, and quickstart.
    -   `Docs/Operations.md` — running, monitoring, troubleshooting.
    -   `Docs/Contributing.md` — contribution guidelines and workflows.
    -   `Docs/ReleaseNotes.md` — release notes and upgrade guides.
-   **Style:** Prefer clear headings, short paragraphs, and code fences for commands or examples. Use relative links between docs.
-   **Versioning:** If docs differ by version, note the applicable app version at the top of each file.
-   **Ownership:** Each new feature must add or update relevant documents in `Docs/` as part of the same PR.


[← Back to README](../README.md)
