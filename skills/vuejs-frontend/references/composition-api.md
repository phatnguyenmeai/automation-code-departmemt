# Vue 3 Composition API Quick Reference

## Reactivity

| API | Use |
|-----|-----|
| `ref(value)` | Primitive reactivity (`.value` access in script) |
| `reactive(obj)` | Object reactivity (direct property access) |
| `computed(() => ...)` | Derived reactive value |
| `watch(source, cb)` | Watch a reactive source |
| `watchEffect(() => ...)` | Auto-track dependencies |
| `toRef(obj, 'key')` | Create ref from reactive property |
| `toRefs(obj)` | Destructure reactive object preserving reactivity |

## Lifecycle Hooks

| Hook | Timing |
|------|--------|
| `onBeforeMount` | Before DOM mount |
| `onMounted` | After DOM mount — safe for DOM access |
| `onBeforeUpdate` | Before re-render |
| `onUpdated` | After re-render |
| `onBeforeUnmount` | Before teardown — cleanup here |
| `onUnmounted` | After teardown |

## Component Communication

```vue
<!-- Parent -->
<ChildComponent
  :title="title"
  @update="handleUpdate"
  v-model="selected"
/>

<!-- Child -->
<script setup lang="ts">
const props = defineProps<{ title: string }>()
const emit = defineEmits<{ update: [value: string] }>()
const model = defineModel<string>()  // v-model binding
</script>
```

## Provide/Inject (Dependency Injection)

```typescript
// Parent
const theme = ref('dark')
provide('theme', theme)

// Deep child
const theme = inject<Ref<string>>('theme', ref('light'))
```

## Template Refs

```vue
<script setup lang="ts">
import { ref, onMounted } from 'vue'

const inputEl = ref<HTMLInputElement | null>(null)

onMounted(() => {
  inputEl.value?.focus()
})
</script>

<template>
  <input ref="inputEl" />
</template>
```
