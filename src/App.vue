<script setup lang="ts">
import { ref, onMounted, onErrorCaptured } from 'vue'
import MainLayout from './components/MainLayout.vue'
import ApiKeyInput from './components/ApiKeyInput.vue'
import { load } from '@tauri-apps/plugin-store'

const hasApiKey = ref(false)
const loading = ref(true)
const appError = ref<Error | null>(null)

// Add global error handler
onErrorCaptured((err, instance, info) => {
  console.error('Application error captured:', err, info);
  appError.value = err instanceof Error ? err : new Error(String(err));
  loading.value = false;
  return false; // prevent error from propagating further
})

onMounted(() => {
  console.log('App component mounted');
  
  // Add timeout to prevent infinite loading
  const loadingTimeout = setTimeout(() => {
    if (loading.value) {
      console.warn('Loading timeout reached, forcing app to show');
      loading.value = false;
    }
  }, 5000);

  // Initialize app with promise timeout
  const initializeApp = async () => {
    try {
      const storePromise = load('.settings.dat');
      
      // Add timeout to the store loading
      const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(() => {
          reject(new Error('Timeout: Loading settings took too long'));
        }, 3000);
      });
      
      // Race between actual loading and timeout
      const store = await Promise.race([storePromise, timeoutPromise]);
      
      // Type assertion for store to fix "store is of type unknown" error
      const apiKey = await (store as any).get('openai-api-key');
      hasApiKey.value = !!apiKey;
      
    } catch (error) {
      console.error('Error initializing app:', error);
      // Continue anyway - we'll just prompt for API key
      hasApiKey.value = false;
    } finally {
      loading.value = false;
      clearTimeout(loadingTimeout);
    }
  };
  
  // Start initialization non-blocking
  setTimeout(() => {
    initializeApp().catch(err => {
      console.error('Unhandled error during initialization:', err);
      loading.value = false;
    });
  }, 0);
})

const handleApiKeySet = () => {
  hasApiKey.value = true
}

// Add retry functionality
const retryInitialization = () => {
  appError.value = null;
  loading.value = true;
  
  setTimeout(() => {
    location.reload();
  }, 500);
}
</script>

<template>
  <div class="app-container">
    <!-- Loading state -->
    <div v-if="loading" class="loading-container">
      <div class="loading-spinner"></div>
      <p>Загрузка приложения...</p>
    </div>
    
    <!-- Error state -->
    <div v-else-if="appError" class="error-container">
      <h3>Произошла ошибка при запуске приложения</h3>
      <p>{{ appError.message || 'Неизвестная ошибка' }}</p>
      <button @click="retryInitialization" class="retry-button">
        Перезапустить приложение
      </button>
    </div>
    
    <!-- App content -->
    <div v-else>
      <ApiKeyInput v-if="!hasApiKey" @apiKeySet="handleApiKeySet" />
      <MainLayout v-else />
    </div>
  </div>
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

/* Loading styles */
.loading-container {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100vh;
  gap: 20px;
}

.loading-spinner {
  width: 40px;
  height: 40px;
  border: 4px solid rgba(0, 119, 255, 0.3);
  border-top-color: var(--accent-primary);
  border-radius: 50%;
  animation: spin 1s infinite linear;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

/* Error container */
.error-container {
  max-width: 500px;
  margin: 100px auto;
  padding: 30px;
  background-color: var(--background-secondary);
  border-radius: 12px;
  text-align: center;
  border: 1px solid var(--error-color);
}

.error-container h3 {
  margin-bottom: 15px;
  color: var(--error-color);
}

.error-container p {
  margin-bottom: 20px;
  color: var(--text-secondary);
}

.retry-button {
  background-color: var(--error-color);
}

.retry-button:hover {
  opacity: 0.9;
}

.app-container {
  min-height: 100vh;
}
</style>