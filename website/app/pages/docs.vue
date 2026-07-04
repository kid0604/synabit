<script setup lang="ts">
import { marked } from 'marked'
import { computed } from 'vue'

useSeoMeta({
  title: 'Documentation - Synabit',
  description: 'Learn how to use Synabit, the local-first digital brain.'
})

// Trỏ tới GitHub Raw để luôn lấy bản mới nhất khi push lên Github
const githubRawUrl = 'https://raw.githubusercontent.com/kid0604/synabit/main/website/public/content/docs/getting-started.md'
const { data: markdownContent } = await useFetch(githubRawUrl)

const htmlContent = computed(() => {
  if (!markdownContent.value) return 'Loading documentation...'
  return marked.parse(markdownContent.value as string)
})

const links = [
  {
    title: 'Getting Started',
    items: [
      { label: 'Introduction', to: '/docs' },
      { label: 'Installation', to: '/docs' },
      { label: 'Setup P2P Sync', to: '/docs' }
    ]
  },
  {
    title: 'Core Features',
    items: [
      { label: 'Note Vault', to: '/docs' },
      { label: 'Task Management', to: '/docs' },
      { label: 'Whiteboard', to: '/docs' },
      { label: 'Local AI', to: '/docs' }
    ]
  }
]
</script>

<template>
  <UContainer class="py-12">
    <div class="grid grid-cols-1 md:grid-cols-4 gap-8">
      <!-- Sidebar Navigation -->
      <aside class="md:col-span-1 border-r border-slate-200 dark:border-slate-800 pr-4">
        <nav class="sticky top-24 space-y-8">
          <div v-for="group in links" :key="group.title">
            <h4 class="font-semibold text-slate-900 dark:text-slate-200 mb-3">{{ group.title }}</h4>
            <ul class="space-y-2">
              <li v-for="item in group.items" :key="item.label">
                <NuxtLink :to="item.to" class="text-sm text-slate-600 hover:text-cyan-600 dark:text-slate-400 dark:hover:text-cyan-400 transition-colors">
                  {{ item.label }}
                </NuxtLink>
              </li>
            </ul>
          </div>
        </nav>
      </aside>

      <!-- Main Content -->
      <main class="md:col-span-2 prose prose-slate dark:prose-invert prose-cyan max-w-none" v-html="htmlContent"></main>

      <!-- Table of Contents (On this page) -->
      <aside class="hidden md:block md:col-span-1 pl-4">
        <div class="sticky top-24">
          <h4 class="font-semibold text-slate-900 dark:text-slate-200 mb-3 text-sm uppercase tracking-wider">On this page</h4>
          <ul class="space-y-2 border-l-2 border-slate-200 dark:border-slate-800 pl-4">
            <li><a href="#" class="text-sm text-cyan-600 dark:text-cyan-400 font-medium">Introduction</a></li>
            <li><a href="#" class="text-sm text-slate-600 hover:text-slate-900 dark:text-slate-400 dark:hover:text-slate-200">Why Local-First?</a></li>
            <li><a href="#" class="text-sm text-slate-600 hover:text-slate-900 dark:text-slate-400 dark:hover:text-slate-200">Serverless P2P Sync</a></li>
          </ul>
        </div>
      </aside>
    </div>
  </UContainer>
</template>


