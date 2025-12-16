Create a GitHub issue with proper context.

Arguments: $ARGUMENTS (required: brief description of the problem)

1. **Understand the problem**: Parse the description from arguments
2. **Gather context**:
   - Search the codebase for relevant files
   - Run commands to reproduce or investigate (e.g., if it's a crash, try to reproduce it)
   - Check for related issues: `gh issue list --search "relevant keywords"`
3. **Ask clarifying questions** if needed:
   - What's the expected behavior?
   - How to reproduce?
   - Any error messages or stack traces?
4. **Draft the issue** with:
   - Clear title
   - Description of the problem
   - Steps to reproduce (if applicable)
   - Relevant code snippets or file references
   - Environment details if relevant
5. **Create the issue**: `gh issue create --title "..." --body "..."`
6. Report the issue URL

Example: `/report benchmark xxx is segfaulting, must debug with valgrind`
→ Investigates the benchmark, tries to reproduce, gathers stack trace, creates issue with all context.
