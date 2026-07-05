<script setup lang="ts">
useSeoMeta({
  title: 'Payment Successful - Synabit',
  description: 'Thank you for your purchase.'
})
</script>

<template>
  <div class="relative py-32 overflow-hidden">
    <!-- Background Glow -->
    <div class="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[600px] h-[600px] bg-green-500/10 rounded-full blur-[120px] pointer-events-none" />

    <UContainer>
      <div class="max-w-2xl mx-auto text-center relative z-10">
        <!-- Success Icon -->
        <div class="mx-auto w-24 h-24 bg-green-500/10 text-green-500 rounded-full flex items-center justify-center mb-8 ring-1 ring-green-500/25 shadow-2xl shadow-green-500/20">
          <UIcon name="i-heroicons-check" class="w-12 h-12" />
        </div>

        <h1 class="text-4xl md:text-5xl font-extrabold text-slate-900 dark:text-white mb-6">
          Thanh toán thành công!
        </h1>
        
        <p class="text-lg text-slate-600 dark:text-slate-300 mb-8 leading-relaxed">
          Cảm ơn bạn đã nâng cấp lên Synabit Pro. Hệ thống đang tự động xử lý đơn hàng của bạn.
        </p>

        <div class="bg-white/50 dark:bg-slate-900/50 backdrop-blur-xl ring-1 ring-slate-200 dark:ring-white/10 rounded-2xl p-8 mb-12 max-w-lg mx-auto text-left shadow-xl shadow-slate-200/20 dark:shadow-black/40">
          <h3 class="text-xl font-bold text-slate-900 dark:text-white mb-4 flex items-center gap-2">
            <UIcon name="i-heroicons-envelope-open" class="w-6 h-6 text-cyan-500" />
            Bước tiếp theo
          </h3>
          <ul class="space-y-4">
            <li class="flex gap-3 text-slate-600 dark:text-slate-300">
              <span class="flex-shrink-0 w-6 h-6 rounded-full bg-cyan-100 dark:bg-cyan-500/20 text-cyan-600 dark:text-cyan-400 flex items-center justify-center text-sm font-bold">1</span>
              <span>Kiểm tra hộp thư email của bạn (bao gồm cả thư mục Spam/Junk).</span>
            </li>
            <li class="flex gap-3 text-slate-600 dark:text-slate-300">
              <span class="flex-shrink-0 w-6 h-6 rounded-full bg-cyan-100 dark:bg-cyan-500/20 text-cyan-600 dark:text-cyan-400 flex items-center justify-center text-sm font-bold">2</span>
              <span>Copy <strong>License Key</strong> đính kèm trong email.</span>
            </li>
            <li class="flex gap-3 text-slate-600 dark:text-slate-300">
              <span class="flex-shrink-0 w-6 h-6 rounded-full bg-cyan-100 dark:bg-cyan-500/20 text-cyan-600 dark:text-cyan-400 flex items-center justify-center text-sm font-bold">3</span>
              <span>Mở ứng dụng Synabit Desktop, vào Cài đặt > Bản quyền và dán mã để kích hoạt.</span>
            </li>
          </ul>
        </div>

        <div class="flex flex-col sm:flex-row items-center justify-center gap-4">
          <UButton 
            to="/" 
            size="lg" 
            color="neutral" 
            variant="ghost"
            class="ring-1 ring-slate-200 dark:ring-slate-700 hover:bg-slate-50 dark:hover:bg-slate-800 rounded-full px-8"
          >
            Về trang chủ
          </UButton>
        </div>
      </div>
    </UContainer>
  </div>
</template>
