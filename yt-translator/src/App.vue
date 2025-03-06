<script setup lang="ts">
import MainLayout from './components/MainLayout.vue'
</script>

<template>
  <MainLayout />
</template>

<style>
:root {
  /* Using system font stack with SF Pro for macOS */
  font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Text', 'Helvetica Neue',
    Arial, sans-serif;
  font-size: 16px;
  line-height: 1.5;
  font-weight: 400;

  /* Light theme colors */
  --text-primary: #1d1d1f;
  --text-secondary: #86868b;
  --background-primary: #f5f5f7;
  --background-secondary: rgba(255, 255, 255, 0.8);
  --accent-primary: #0071e3;
  --accent-secondary: #147ce5;
  --border-color: rgba(0, 0, 0, 0.1);
  --shadow-color: rgba(0, 0, 0, 0.1);
  --error-color: #ff3b30;

  color: var(--text-primary);
  background-color: var(--background-primary);

  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  -webkit-text-size-adjust: 100%;
}

@media (prefers-color-scheme: dark) {
  :root {
    /* Dark theme colors */
    --text-primary: #f5f5f7;
    --text-secondary: #86868b;
    --background-primary: #1d1d1f;
    --background-secondary: rgba(40, 40, 40, 0.8);
    --accent-primary: #2997ff;
    --accent-secondary: #147ce5;
    --border-color: rgba(255, 255, 255, 0.1);
    --shadow-color: rgba(0, 0, 0, 0.3);
    --error-color: #ff453a;
  }
}

* {
  box-sizing: border-box;
  margin: 0;
  padding: 0;
}

body {
  min-height: 100vh;
  background-color: var(--background-primary);
}

/* Common button styles */
button {
  background-color: var(--accent-primary);
  color: white;
  border: none;
  border-radius: 980px; /* Apple's rounded button style */
  padding: 8px 16px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
}

button:hover {
  background-color: var(--accent-secondary);
  transform: scale(1.02);
}

button:active {
  transform: scale(0.98);
}

/* Common input styles */
input, select {
  background-color: var(--background-secondary);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  padding: 8px 16px;
  font-size: 14px;
  color: var(--text-primary);
  transition: all 0.2s ease;
}

input:focus, select:focus {
  outline: none;
  border-color: var(--accent-primary);
  box-shadow: 0 0 0 2px var(--accent-primary);
}

/* Common backdrop blur for floating elements */
.backdrop-blur {
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
}
</style>