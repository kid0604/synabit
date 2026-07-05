<script setup lang="ts">
import { ref } from 'vue'

useSeoMeta({
  title: 'Pricing - Synabit',
  description: 'Choose the best plan for your digital brain.'
})

const isYearly = ref(true)

const plans = [
  {
    name: 'Free (Open Source)',
    description: 'Build it yourself from source code. Perfect for developers.',
    priceMonthly: 0,
    priceYearly: 0,
    features: [
      'Full source code on GitHub',
      'No feature limitations',
      'Self-hosted P2P Sync',
      'Community Support'
    ],
    buttonText: 'View on GitHub',
    buttonColor: 'neutral',
    buttonVariant: 'outline',
    buttonTo: 'https://github.com/kid0604/synabit'
  },
  {
    name: 'Pro',
    description: 'Pre-built binaries, automatic updates, and premium support.',
    priceMonthly: 5,
    priceYearly: 50,
    badge: 'Most Popular',
    features: [
      'Everything in Free',
      'Managed Sync Server',
      '100GB Cloud Data Storage',
      'Automatic Updates',
      'Premium Support'
    ],
    buttonText: 'Subscribe Now',
    buttonColor: 'primary',
    buttonVariant: 'solid',
    buttonTo: 'https://buy.polar.sh/polar_cl_xjks376haGFsjPFNyh2uRpAV4TfG4orQnHiQS4ERlye'
  }
]
</script>

<template>
  <div class="relative py-24">
    <!-- Background Glow -->
    <div class="absolute top-0 left-1/2 -translate-x-1/2 w-[600px] h-[600px] bg-cyan-500/10 rounded-full blur-[120px] pointer-events-none" />

    <UContainer>
      <div class="text-center max-w-2xl mx-auto mb-16 relative z-10">
        <h1 class="text-4xl font-bold text-slate-900 dark:text-white mb-4">Simple, transparent pricing</h1>
        <p class="text-lg text-slate-600 dark:text-slate-400 mb-8">
          Unlock the full potential of your digital brain. No hidden fees, cancel anytime.
        </p>
        
        <!-- Toggle Monthly / Yearly -->
        <div class="inline-flex items-center gap-4 bg-white/50 dark:bg-slate-900/50 p-2 rounded-full ring-1 ring-slate-200 dark:ring-white/10 backdrop-blur-md">
          <button
            @click="isYearly = false"
            :class="['px-6 py-2 rounded-full text-sm font-medium transition-all duration-200', !isYearly ? 'bg-cyan-500 text-white shadow-lg shadow-cyan-500/25' : 'text-slate-600 hover:text-slate-900 dark:text-slate-400 dark:hover:text-white']"
          >
            Monthly
          </button>
          <button
            @click="isYearly = true"
            :class="['px-6 py-2 rounded-full text-sm font-medium transition-all duration-200', isYearly ? 'bg-cyan-500 text-white shadow-lg shadow-cyan-500/25' : 'text-slate-600 hover:text-slate-900 dark:text-slate-400 dark:hover:text-white']"
          >
            Yearly <span class="text-xs text-cyan-200 ml-1">Save 15%</span>
          </button>
        </div>
      </div>

      <div class="grid grid-cols-1 md:grid-cols-2 gap-8 max-w-4xl mx-auto relative z-10">
        <div 
          v-for="plan in plans" 
          :key="plan.name"
          class="relative flex flex-col p-8 rounded-3xl bg-white/40 dark:bg-slate-900/40 ring-1 backdrop-blur-xl transition-all duration-300 hover:-translate-y-1"
          :class="plan.name === 'Pro' ? 'ring-cyan-500/50 shadow-2xl shadow-cyan-500/10' : 'ring-slate-200 dark:ring-white/10'"
        >
          <div v-if="plan.badge" class="absolute -top-4 left-1/2 -translate-x-1/2 px-4 py-1 bg-gradient-to-r from-cyan-500 to-blue-500 text-white text-xs font-bold rounded-full shadow-lg">
            {{ plan.badge }}
          </div>

          <div class="mb-8">
            <h3 class="text-2xl font-bold text-slate-900 dark:text-white mb-2">{{ plan.name }}</h3>
            <p class="text-slate-600 dark:text-slate-400 text-sm h-10">{{ plan.description }}</p>
          </div>

          <div class="mb-8">
            <div class="flex items-baseline gap-1">
              <span class="text-5xl font-extrabold text-slate-900 dark:text-white">
                ${{ isYearly ? plan.priceYearly : plan.priceMonthly }}
              </span>
              <span class="text-slate-400 font-medium">/{{ isYearly ? 'year' : 'month' }}</span>
            </div>
            <p v-if="isYearly && plan.priceYearly > 0" class="text-sm text-cyan-400 mt-2">
              Save ~16% compared to monthly
            </p>
          </div>

          <ul class="space-y-4 mb-8 flex-1">
            <li v-for="feature in plan.features" :key="feature" class="flex items-start gap-3">
              <UIcon name="i-heroicons-check-circle" class="w-5 h-5 text-cyan-400 shrink-0 mt-0.5" />
              <span class="text-slate-700 dark:text-slate-300 text-sm">{{ feature }}</span>
            </li>
          </ul>

          <UButton 
            block 
            size="lg" 
            :to="plan.buttonTo"
            :target="plan.buttonTo && plan.buttonTo.startsWith('http') ? '_blank' : undefined"
            :color="plan.buttonColor as any" 
            :variant="plan.buttonVariant as any"
            class="mt-auto rounded-full font-semibold"
          >
            {{ plan.buttonText }}
          </UButton>
        </div>
      </div>
    </UContainer>
  </div>
</template>
