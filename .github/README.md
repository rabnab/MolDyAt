# GitHub Copilot Instructions Setup

This repository is configured to work with GitHub Copilot across multiple platforms. The instructions are consolidated to avoid duplication and ensure consistency.

## File Structure

```
.github/
├── copilot-instructions.md      # Main instructions file (source of truth)
├── README.md                     # This file - explains the setup
└── instructions/
    └── Common_rules             # RustRover quick reference (links to main file)
```

The `Common_rules` file provides a quick reference for RustRover users but references the main `copilot-instructions.md` for complete details, avoiding duplication.

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
- **Note**: This file provides a quick reference and links to the main `copilot-instructions.md` for complete details

## Updating Instructions

When updating Copilot instructions:

1. **Edit the main file**: `.github/copilot-instructions.md` (source of truth)
2. **RustRover users automatically get updates**: The `Common_rules` file references the main file
3. **No manual syncing required**: The reference-based approach eliminates duplication

### Important Workflow Rules

⚠️ **Always use pull requests** - Never commit directly to master  
✅ **Create a feature branch** for all changes  
✅ **Run tests and formatting** before pushing  
✅ **Wait for CI and reviews** before merging

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

