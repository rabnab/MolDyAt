# GitHub Copilot Instructions Setup

This repository is configured to work with GitHub Copilot across multiple platforms. The instructions are consolidated to avoid duplication and ensure consistency.

## File Structure

```
.github/
├── copilot-instructions.md      # Main instructions file (for GitHub Copilot Web & Mobile)
└── instructions/
    └── Common_rules             # RustRover/JetBrains IDE instructions (same content)
```

## Platform Support

### ✅ GitHub Copilot Web
- **Location**: `.github/copilot-instructions.md`
- **Status**: Configured
- **Access**: Automatically loaded when using Copilot in github.com

### ✅ GitHub Copilot Mobile (Android/iOS)
- **Location**: `.github/copilot-instructions.md`
- **Status**: Configured
- **Access**: Automatically loaded in the GitHub mobile app

### ✅ RustRover / JetBrains IDEs
- **Location**: `.github/instructions/Common_rules`
- **Status**: Configured
- **Access**: Loaded via JetBrains Copilot plugin
- **Note**: This file contains the same content as `copilot-instructions.md`

## Updating Instructions

When updating Copilot instructions:

1. **Edit the main file**: `.github/copilot-instructions.md`
2. **Sync to RustRover**: Copy the content to `.github/instructions/Common_rules`

Or use this PowerShell command to sync:
```powershell
Copy-Item .github\copilot-instructions.md .github\instructions\Common_rules
```

## GitHub Copilot Features

This repository leverages:
- **Context-aware code suggestions** based on project-specific rules
- **Physics domain knowledge** for Lennard-Jones simulations
- **Rust best practices** and nightly toolchain awareness
- **Energy conservation constraints** for physics calculations

## Testing the Setup

To verify Copilot is loading the instructions:

1. **In RustRover**: Ask Copilot "What physics constraint is most important in this project?"
2. **In GitHub Web**: Open any file and ask Copilot the same question
3. **Expected answer**: "Energy conservation"

## References

- [GitHub Copilot coding agent best practices](https://gh.io/copilot-coding-agent-tips)
- [JetBrains Copilot documentation](https://www.jetbrains.com/help/idea/copilot.html)

