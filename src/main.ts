import { createPinia } from "pinia"
import { createApp } from "vue"

import { applyTheme } from "@/lib/theme"

import App from "./App.vue"
import router from "./router"

import "vue-sonner/style.css"
import "./assets/css/main.css"

// 先落一个默认主题，避免首帧白闪
applyTheme("system")

createApp(App).use(createPinia()).use(router).mount("#app")
