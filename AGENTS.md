# AGENTS.md

## Build & Dev Commands

- `bun dev` - Start Vite dev server (frontend only)
- `bun tauri dev` - Start Tauri app with hot reload
- `bun build` - Build frontend; `bun tauri build` - Build full app
- `bun check` - TypeScript/Svelte type checking
- `cargo check --manifest-path src-tauri/Cargo.toml` - Check Rust code

## Code Style

- **TypeScript**: Strict mode enabled. Use explicit types, avoid `any`.
- **Svelte 5**: Use `$state()`, `$props()`, `$bindable()` runes. Use `{@render children?.()}` for slots.
- **Imports**: Use `$lib/` alias for lib imports. Import UI components from `$lib/components/ui/<component>`.
- **Styling**: Use Tailwind CSS with `cn()` from `$lib/utils` for conditional classes.
- **Naming**: camelCase for variables/functions, PascalCase for components/types.

## UI Components (shadcn-svelte)

**Always use shadcn-svelte components from `$lib/components/ui/`** for a polished UI:
Button, Card, Dialog, Sheet, Tabs, Input, Select, Badge, Tooltip, Sonner (toasts), Spinner, etc.
Example: `import { Button } from "$lib/components/ui/button";`
Icons: Use `@lucide/svelte` - `import { IconName } from "@lucide/svelte/icons/icon-name";`

## Tauri (Rust)

- Commands in `src-tauri/src/lib.rs`. Use `#[tauri::command]` macro.
- Call from frontend: `import { invoke } from "@tauri-apps/api/core"; await invoke("command_name", { args });`
