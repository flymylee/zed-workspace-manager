# Zed Workspace Manager

A lightweight GUI database inspector and SQL query builder for the [Zed](https://zed.dev) editor's local SQLite workspace database (`db.sqlite`), written in Rust with [`egui`](https://github.com/emilk/egui) and [`rusqlite`](https://github.com/rusqlite/rusqlite).

## Why?

Zed stores workspace history, paths, and session metadata in a local SQLite database. Modifying that database directly while Zed is running can cause lock contention or unintended data loss. **Zed Workspace Manager** opens the database strictly in **read-only mode** (`SQLITE_OPEN_READ_ONLY`), lets you inspect and filter entries safely, and generates ready-to-run `SELECT` and `DELETE` SQL queries with `WHERE ... IN (...)` clauses for you to review and execute in Zed or `sqlite3`.

## Features

- **Lock-Safe Read-Only Inspection**: Safely reads `db.sqlite` even while Zed is actively running.
- **Auto DB Detection**: Automatically locates Zed's default `0-global/db.sqlite` or `0-stable/db.sqlite` across Linux, macOS, and Windows (with manual path override and instant refresh).
- **Live Keyword Filtering**: Filter rows by workspace path or keyword in real time.
- **Batch Selection Controls**: Select All (respects active filter), Invert Selection, and Deselect All.
- **One-Click SQL Query Builder**:
  - **All SELECT SQL**: Generates a full `SELECT` query ordered by primary key.
  - **Selected SELECT SQL**: Generates a `SELECT ... WHERE <pk> IN (...)` query for checked rows.
  - **Selected DELETE SQL**: Generates a `DELETE ... WHERE <pk> IN (...)` query paired with a commented safety preview `SELECT` statement, plus one-click clipboard copy.

---

## Quick Start (Linux `x86_64` AppImage)

Pre-built `x86_64` Linux AppImage binaries can be downloaded from the [Releases](../../releases) page (or generated via the **Actions** tab).

```bash
chmod +x zed-workspace-manager-0.1.0-x86_64.AppImage
./zed-workspace-manager-0.1.0-x86_64.AppImage
```
