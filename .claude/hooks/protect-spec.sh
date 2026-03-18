#!/bin/bash
INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')

if [[ "$FILE_PATH" == */spec/* || "$FILE_PATH" == */spec ]]; then
  echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow","additionalContext":"IMPORTANT: You are about to modify a file in the spec/ directory. These are read-only project specification documents. You MUST confirm with the user before proceeding with this modification. Ask the user for explicit approval first."}}'
  exit 0
fi

exit 0
