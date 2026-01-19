# Claude Bar - Autonomous Development Session

You are working on `claude-bar`, a Linux system tray app for monitoring AI usage limits.

## Your Task

1. **Read SPEC.md** to understand the project and find the next incomplete phase
2. **Pick the highest-priority incomplete phase** (Phase 3 comes before Phase 4, etc.)
3. **Implement that phase completely**:
   - Follow the checklist items in order
   - Reference the CodexBar Swift implementation at `/vault/experiments/2026-01-16-steipete-CodexBar/` when porting logic
   - Write tests for new functionality
4. **Verify your work**:
   - Run `cargo check` - must pass
   - Run `cargo test` - must pass
   - Run `cargo clippy` - address any warnings
5. **Update SPEC.md**:
   - Check off completed items (`- [ ]` → `- [x]`)
   - Add a "Phase N Notes" section with implementation details useful for future sessions
   - Note any deviations from the original plan
6. **Commit your work** with a descriptive message

## Important Guidelines

- Only work on ONE phase per session
- If you encounter a blocker, add a `> BLOCKED:` note in SPEC.md and move on
- Don't modify code from previous phases unless fixing a bug
- Keep the codebase building at all times
- Reference AGENTS.md for coding conventions

## When You're Done

After committing, you're done. Exit cleanly. The next session will pick up the next phase.
