#!/bin/bash
# Scaffold a new Vue.js page with component, route, and store.
# Usage: ./scaffold-page.sh <PageName> [route-path]
#
# Example: ./scaffold-page.sh UserProfile /users/:id
#
# Creates:
#   src/pages/<page-name>/
#   ├── <PageName>Page.vue
#   └── components/

set -euo pipefail

PAGE_NAME="${1:?Usage: scaffold-page.sh <PageName> [route-path]}"
ROUTE_PATH="${2:-/$(echo "$PAGE_NAME" | sed 's/\([A-Z]\)/-\L\1/g' | sed 's/^-//')}"
DIR_NAME=$(echo "$PAGE_NAME" | sed 's/\([A-Z]\)/-\L\1/g' | sed 's/^-//')
PAGE_DIR="src/pages/${DIR_NAME}"

mkdir -p "$PAGE_DIR/components"

cat > "$PAGE_DIR/${PAGE_NAME}Page.vue" << EOF
<script setup lang="ts">
import { ref, onMounted } from 'vue'

// Props & emits
// const props = defineProps<{}>()
// const emit = defineEmits<{}>()

// State
const loading = ref(true)

// Lifecycle
onMounted(async () => {
  try {
    // TODO: fetch data
  } finally {
    loading.value = false
  }
})
</script>

<template>
  <div class="${DIR_NAME}-page">
    <h1>${PAGE_NAME}</h1>
    <div v-if="loading" data-testid="loading-spinner">
      Loading...
    </div>
    <div v-else>
      <!-- TODO: page content -->
    </div>
  </div>
</template>

<style scoped>
.${DIR_NAME}-page {
  padding: 1rem;
}
</style>
EOF

echo "Page scaffolded at $PAGE_DIR/${PAGE_NAME}Page.vue"
echo "Route path: $ROUTE_PATH"
echo ""
echo "Add to router/index.ts:"
echo "  {"
echo "    path: '${ROUTE_PATH}',"
echo "    component: () => import('@/pages/${DIR_NAME}/${PAGE_NAME}Page.vue'),"
echo "  }"
