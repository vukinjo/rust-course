# Rust for Experienced Developers

A Rust course for developers who already know Kotlin or Java, C, operating systems and design patterns. It
goes from first principles to a REST microservice with PostgreSQL. Each topic maps Rust back to what you know:
Kotlin for high-level features, and C for the low-level ones the JVM hides.

## What's here

```
course/              the course: one offline HTML page
  index.html         27 chapters, appendices A–F, practice tasks with solutions
  fonts/             the page's fonts, so it works without a network
project/             the service the course builds, as it stands after the last chapter
  integration-tests/ a Bruno collection that tests the running service
```

## The course

Open `course/index.html` in a browser. It needs no server and no internet connection; only the
**Run in Playground** buttons go online, to the official Rust Playground.

| Part | Chapters |
|---|---|
| I. Foundations | 1 Getting started (installing Rust in WSL), 2 Variables, types and functions, 3 Control flow |
| II. Ownership | 4 Ownership and moves, 5 Borrowing and slices, 6 Lifetimes |
| III. Modeling data | 7 Structs, 8 Enums and pattern matching, 9 Error handling, 10 Modules |
| IV. Abstraction | 11 Traits, 12 Generics, 13 Closures and iterators, 14 Collections |
| V. Memory and concurrency | 15 Smart pointers, 16 Concurrency |
| VI. Macros, testing and I/O | 17 Macros, 18 Testing and documentation, 19 Files, processes and networking |
| VII. Building the service | 20 serde, 21 Tokio, 22 axum, 23 reqwest, 24 tracing, 25 PostgreSQL with SQLx, 26 Schema changes, 27 Authentication |
| Appendices | A Rc and RefCell, B More concurrency, C Async from the standard library up, D Unsafe and FFI, E A key-value server, F Design patterns |

- **One project throughout.** Alongside the short examples, you build one Cargo project, `shoes`, a catalog
  of shoes with variants and versions. It starts as one printed product in Chapter 1 and becomes a web service
  in Chapters 22–27. Each *project step* shows the files it changes, in full or as a diff.
- **Tested.** Every example is a complete program, and the output under it was captured from a real run.
  Examples marked *does not compile* show the real compiler error.
- **Practice.** Each chapter ends with practice tasks. Every task has a solution, collapsed under it, that was
  compiled and run (or, for the project chapters, applied to that chapter's code and tested).
- **Setup.** Rust 1.100, edition 2024, on Ubuntu 26.04 in WSL 2; Chapters 25–27 use PostgreSQL 18 in Podman.
  Chapter 1 starts from a machine with no Rust installed. macOS notes are included, but weren't tested on a Mac.

## The project

`project/` is the `shoes` service after Chapter 27: a REST API with axum, PostgreSQL through SQLx with
migrations, a reqwest client for reviews, structured logs with tracing, layered settings, and a bearer-token
check on every request. Its [README](project/README.md) explains how to start the database, run the service and
run the tests (39 Rust tests, and the Bruno collection's 37 requests).

Quick start, once the PostgreSQL container from Chapter 25 is running:

```bash
cd project
cargo run                                                        # listens on 127.0.0.1:3000
curl -H 'Authorization: Bearer abcdefghij' localhost:3000/products   # any 10-character token
```
