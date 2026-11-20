# StudyNodes: Visual Knowledge & LLM Synthesis Canvas

> A native GPU-rendered desktop studying application built with **Rust** and the **Rune** framework (`anomalyco/rune`), inspired by Blender's node editor, Antigravity, and Claude Code.

---

## Overview

**StudyNodes** is an infinite 2D visual thinking and study tool where learners can drag and drop documents, research notes, and images onto a canvas, connect them with annotated Bézier curve wires, and collaborate with an LLM to discover latent semantic relationships or synthesize cross-document insights.

---

## Key Features

1. **Project Dashboard**:
   - Create, open, and view existing study projects.
   - Quick-start study templates (*Literature Review*, *Exam Prep*, *Blank Canvas*).
   - Project cards displaying node counts, edge counts, and folder metadata.

2. **Blender-Style Infinite Node Graph**:
   - **GPU-Rendered Canvas**: 120Hz smooth rendering powered by Rune, wgpu, and DirectX 12.
   - **Camera Math**: Pan and zoom anchored to cursor coordinates.
   - **Cubic Bézier Wires**: S-curve connections matching Blender's node graph aesthetic.
   - **Midpoint Annotation Pills**: Interactive relation badges (`supports`, `contradicts`, `derived_from`, `explains`) and relationship notes.
   - **Node Dragging & Repositioning**: Real-time gesture tracking via Rune's `Phase` state machine.

3. **Multi-Modal Content Ingestion**:
   - Drag and drop documents (PDF, Markdown, text) and images (PNG, JPEG, SVG) directly into the canvas.
   - Ingested files are automatically hashed with SHA-256 and stored in the project's content-addressed `assets/` directory.

4. **Human-in-the-Loop LLM Synthesis**:
   - **Bounded Context Packaging**: Packs selected focus nodes, 1-hop neighbors, and existing edges into an LLM prompt.
   - **Structured `GraphPatch` Schema**: LLM proposals are validated as typed JSON diffs (`propose_connection`, `create_synthesis_node`, `annotate_edge`, `update_node_summary`).
   - **Ghost Edges**: Proposed connections appear as glowing violet wires with inline `[✓ Accept]` and `[✕ Dismiss]` buttons.

5. **Crash-Resilient Atomic Persistence**:
   - Each project is a self-contained bundle directory (`MyTopic.studynodes/`).
   - Atomic saving: writes to `project.json.tmp`, flushes with `sync_all()`, and atomically renames to `project.json`.
   - Schema version migration guards and automatic orphan asset pruning.

---

## Workspace Structure

```
StudyNodes/
├── Cargo.toml
├── crates/
│   ├── studynodes-graph/        # UI-agnostic core graph data structures & math
│   │   ├── src/coords.rs        # WorldPos, ScreenPos, Camera pan/zoom
│   │   ├── src/model.rs         # Node, Port, Edge, EdgeRelation, EdgeStatus
│   │   ├── src/bezier.rs        # Cubic Bézier S-curves & screen-space hit testing
│   │   ├── src/graph.rs         # StudyGraph topology, 1-hop subgraphs
│   │   ├── src/patch.rs         # GraphPatch schema validation & temp_id staging
│   │   ├── src/command.rs       # Undo/Redo command history
│   │   └── src/bundle.rs        # Atomic project bundles & asset management
│   ├── studynodes-llm/          # Bounded context packing & Gemini API integration
│   │   ├── src/context.rs       # Neighborhood serialization
│   │   ├── src/prompt.rs        # Gemini Neon knowledge synthesis system prompt
│   │   └── src/client.rs        # MockLlmProvider & GeminiLlmProvider
│   └── studynodes-app/          # Native Rune GPU desktop application
│       ├── src/state.rs         # AppState, CanvasMode FSM
│       ├── src/dashboard_view.rs# Dashboard UI (recent list, creation templates)
│       ├── src/canvas_view.rs   # Infinite grid, Bézier wires, node cards
│       ├── src/inspector_view.rs# Side-panel inspector & AI synthesis feed
│       ├── src/main.rs          # AppSpec entry point & headless scenes
│       └── tests/
│           └── e2e_simulation.rs# End-to-end full lifecycle integration test
```

---

## Running the Application

### Interactive Window
```bash
cargo run -p studynodes-app
```

### Headless Visual Snapshots
```bash
# Capture Dashboard:
cargo run -p studynodes-app -- --snap dashboard.png --scene dashboard

# Capture Workspace Node Graph:
cargo run -p studynodes-app -- --snap workspace.png --scene workspace
```

### Run All Tests
```bash
cargo test --workspace
```
