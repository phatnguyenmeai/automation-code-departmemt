#!/bin/bash
# Scaffold a new Vue.js composable function.
# Usage: ./scaffold-composable.sh <composableName>
#
# Example: ./scaffold-composable.sh useUserProfile
#
# Creates: src/composables/useUserProfile.ts

set -euo pipefail

NAME="${1:?Usage: scaffold-composable.sh <composableName>}"

# Ensure name starts with 'use'
if [[ ! "$NAME" == use* ]]; then
    echo "Warning: Composable names should start with 'use' (convention)"
fi

FILE="src/composables/${NAME}.ts"
mkdir -p "src/composables"

cat > "$FILE" << EOF
import { ref, computed, type Ref } from 'vue'

interface ${NAME}Options {
  // TODO: define options
}

interface ${NAME}Return {
  // TODO: define return type
  loading: Ref<boolean>
  error: Ref<string | null>
}

export function ${NAME}(options?: ${NAME}Options): ${NAME}Return {
  const loading = ref(false)
  const error = ref<string | null>(null)

  // TODO: implement composable logic

  return {
    loading,
    error,
  }
}
EOF

echo "Composable created at $FILE"
