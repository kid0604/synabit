<script setup lang="ts">
useSeoMeta({
  title: 'Synabit - Your Ultimate Digital Brain',
  description: 'Synabit is a modern, ultra-fast, local-first productivity workspace. Designed to be your ultimate digital brain.'
})
</script>

<template>
  <div class="relative">
    <!-- Background Graph Pattern / Glow -->
    <div class="absolute inset-0 pointer-events-none z-[-1] overflow-hidden">
      <div class="absolute top-[-20%] left-1/2 -translate-x-1/2 w-[800px] h-[800px] bg-cyan-500/20 rounded-full blur-[120px] opacity-50" />
      <div class="absolute top-40 left-20 w-72 h-72 bg-blue-500/20 rounded-full blur-[100px] opacity-40" />
    </div>

    <!-- Hero Section -->
    <UContainer class="pt-24 pb-32 text-center">
      <div class="inline-flex items-center gap-2 px-3 py-1 rounded-full bg-cyan-500/10 text-cyan-400 text-sm font-medium mb-8 ring-1 ring-cyan-500/20">
        <UIcon name="i-heroicons-sparkles" class="w-4 h-4" />
        <span>Synabit 0.6.0 is out now!</span>
      </div>
      
      <h1 class="text-5xl md:text-7xl font-bold tracking-tight mb-6 bg-gradient-to-r from-slate-900 via-slate-700 to-slate-500 dark:from-white dark:via-slate-200 dark:to-slate-400 bg-clip-text text-transparent">
        Your Ultimate <br class="hidden md:block" /> Digital Brain
      </h1>
      
      <p class="text-lg md:text-xl text-slate-600 dark:text-slate-400 max-w-2xl mx-auto mb-10">
        A modern, ultra-fast, local-first productivity workspace. Unify your notes, tasks, calendar, and ideas into one seamless, cross-platform environment.
      </p>
      
      <div class="flex flex-col sm:flex-row items-center justify-center gap-4">
        <UButton size="xl" color="primary" class="relative group overflow-hidden">
          <span class="absolute inset-0 bg-white/20 group-hover:bg-white/0 transition-colors" />
          <span class="relative flex items-center gap-2">
            Download for Free
            <UIcon name="i-heroicons-arrow-right" class="w-5 h-5 group-hover:translate-x-1 transition-transform" />
          </span>
        </UButton>
        <UButton size="xl" color="neutral" variant="ghost" to="/pricing" class="ring-1 ring-slate-700/50 hover:bg-slate-800/50">
          View Pricing
        </UButton>
      </div>
    </UContainer>

    <!-- Bento Grid Features -->
    <UContainer class="pb-32">
      <div class="grid grid-cols-1 md:grid-cols-3 gap-6">
        <!-- Feature 1 -->
        <div class="md:col-span-2 relative overflow-hidden rounded-2xl bg-white/50 dark:bg-slate-900/50 ring-1 ring-slate-200 dark:ring-white/10 p-8 backdrop-blur-md group hover:ring-cyan-500/30 transition-all duration-300">
          <div class="absolute inset-0 bg-gradient-to-br from-cyan-500/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity" />
          <UIcon name="i-heroicons-rectangle-stack" class="w-10 h-10 text-cyan-400 mb-6" />
          <h3 class="text-2xl font-semibold text-slate-900 dark:text-white mb-3">All-in-One Workspace</h3>
          <p class="text-slate-600 dark:text-slate-400 max-w-md">No more context switching. Manage deep-focus writing, quick fleeting ideas, tasks, and calendar from a single interface.</p>
        </div>

        <!-- Feature 2 -->
        <div class="relative overflow-hidden rounded-2xl bg-white/50 dark:bg-slate-900/50 ring-1 ring-slate-200 dark:ring-white/10 p-8 backdrop-blur-md group hover:ring-cyan-500/30 transition-all duration-300">
          <UIcon name="i-heroicons-lock-closed" class="w-10 h-10 text-cyan-400 mb-6" />
          <h3 class="text-xl font-semibold text-slate-900 dark:text-white mb-3">100% Local & Private</h3>
          <p class="text-slate-600 dark:text-slate-400">Your data belongs to you. Stored locally inside an encrypted database. Zero telemetry.</p>
        </div>

        <!-- Feature 3 -->
        <div class="relative overflow-hidden rounded-2xl bg-white/50 dark:bg-slate-900/50 ring-1 ring-slate-200 dark:ring-white/10 p-8 backdrop-blur-md group hover:ring-cyan-500/30 transition-all duration-300">
          <UIcon name="i-heroicons-arrow-path-rounded-square" class="w-10 h-10 text-cyan-400 mb-6" />
          <h3 class="text-xl font-semibold text-slate-900 dark:text-white mb-3">Serverless P2P Sync</h3>
          <p class="text-slate-600 dark:text-slate-400">Custom peer-to-peer technology securely transfers your data across devices over LAN.</p>
        </div>

        <!-- Feature 4 -->
        <div class="md:col-span-2 relative overflow-hidden rounded-2xl bg-white/50 dark:bg-slate-900/50 ring-1 ring-slate-200 dark:ring-white/10 p-8 backdrop-blur-md group hover:ring-cyan-500/30 transition-all duration-300">
          <div class="absolute inset-0 bg-gradient-to-tl from-cyan-500/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity" />
          <UIcon name="i-heroicons-cpu-chip" class="w-10 h-10 text-cyan-400 mb-6" />
          <h3 class="text-2xl font-semibold text-slate-900 dark:text-white mb-3">Local AI Powered</h3>
          <p class="text-slate-600 dark:text-slate-400 max-w-md">Integrate with your local LLMs (like Ollama) to brainstorm, summarize, and assist you right inside your workspace without compromising privacy.</p>
        </div>
      </div>
    </UContainer>
  </div>
</template>
