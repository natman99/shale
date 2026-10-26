# Shale
Shale is an obsidian-style markdown editor with extra developer focused features. Shale is built with GPUI in Rust

## Goals
- [ ] "Live preview" (obsidian-style) Markdown editor with vim binding support
- [ ] File tree and interface
- [ ] Wiki-links
- [ ] MCP server
- [ ] Spellcheck and grammar checking using HarperLS. Support for custom language servers.
- [ ] Near-perfect compatibility with Obsidian vaults 
- [ ] Image support in vaults
- [ ] Theming support and custom themes. Include light and dark themes.

- Config files in TOML
- Assure any code you reference or copy from online has a commercial permissible licensing 
- Write unit tests 
- Regularly commit to a local git repo. You don't need to publish it. Don't override the user's global config, make a local override for a user for yourself.
- Expand this plan and goals as needed.
- Assure **proper** support for manipulating tables with vim bindings. No unexpected around when manipulating tables. 
- Tree-sitter powered code highlighting for code blocks.

## Non-Goals
- Plugins

### Tech Stack
Keep as much in Rust as possible, no bindings external libraries. Update the tech stack as needed.
- Tokio for async
- GPUI for UI
- Serde for file parsing
- You may want to use GPUI kit, a components library for GPUI. Evaluate if it's useful before using.
https://gpui-kit.com/

### MCP
Shale has an MCP server with support for reading, writing, and searching vaults. The MCP server should *either* be a separate binary which shares core functionality with a `common` crate in the project, or if speed will be acceptable use the main binary.
