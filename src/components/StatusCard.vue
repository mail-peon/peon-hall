<script setup lang="ts">
// 状态卡片：一个状态点 + 标题 + 副标题。
// 按钮在 ActionBar 里 —— 卡片只回答「现在是什么状态」（ai-docs/01-ui-and-states.md § 2）。

const props = defineProps<{
  title: string
  subtitle: string
  /** 状态色名（running / stopped / foreground / none），对应 styles/main.css 的 .state-dot--*。 */
  tone: string
}>()
</script>

<template>
  <section class="card flex items-start gap-3">
    <span class="state-dot mt-1.5" :class="`state-dot--${props.tone}`" />
    <div class="min-w-0">
      <h1 class="text-base font-semibold leading-6">{{ props.title }}</h1>
      <p class="subtitle">{{ props.subtitle }}</p>
    </div>
  </section>
</template>

<style scoped>
.subtitle {
  margin: 0;
  color: var(--muted);
  font-size: 12.5px;
  line-height: 1.5;
}
</style>
