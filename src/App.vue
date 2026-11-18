<script setup lang="ts">
import { Toaster } from "@/components/ui/sonner"
</script>

<template>
  <RouterView />
  <Toaster position="bottom-right" :visible-toasts="3" />
</template>
