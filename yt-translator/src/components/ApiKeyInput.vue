<script setup lang="ts">
import { ref } from 'vue'
import { message } from 'ant-design-vue'
import { invoke } from '@tauri-apps/api/core'
import { load } from '@tauri-apps/plugin-store'

interface Props {
  mode?: 'setup' | 'update'
  onCancel?: () => void
}

const props = withDefaults(defineProps<Props>(), {
  mode: 'setup'
})

const emit = defineEmits(['apiKeySet'])
const apiKey = ref('')
const loading = ref(false)

const validateApiKey = async (key: string): Promise<boolean> => {
  try {
    return await invoke('validate_openai_key', { apiKey: key })
  } catch (error) {
    console.error('Error validating API key:', error)
    return false
  }
}

const handleSubmit = async () => {
  if (!apiKey.value.trim()) {
    message.error('Please enter your OpenAI API key')
    return
  }

  try {
    loading.value = true
    
    // Validate the API key
    const isValid = await validateApiKey(apiKey.value)
    if (!isValid) {
      message.error('Invalid OpenAI API key. Please check and try again.')
      return
    }

    // Store the API key securely
    const store = await load('.settings.dat')
    await store.set('openai-api-key', apiKey.value)
    await store.save()
    
    message.success('API key saved successfully')
    emit('apiKeySet')
  } catch (error) {
    message.error('Failed to save API key')
    console.error('Error saving API key:', error)
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <div class="api-key-container backdrop-blur">
    <div class="api-key-card">
      <h2>{{ mode === 'setup' ? 'OpenAI API Key Setup' : 'Update OpenAI API Key' }}</h2>
      <p>{{ mode === 'setup' 
        ? 'Please enter your OpenAI API key to continue. Your key will be stored securely.' 
        : 'Enter your new OpenAI API key below. The existing key will be replaced.' }}</p>
      <div class="input-group">
        <input
          type="password"
          v-model="apiKey"
          placeholder="Enter your OpenAI API key"
          :disabled="loading"
        />
        <div class="button-group">
          <button 
            v-if="mode === 'update'"
            @click="onCancel"
            :disabled="loading"
            class="secondary"
          >
            Cancel
          </button>
          <button 
            @click="handleSubmit"
            :disabled="loading"
            class="primary"
          >
            {{ loading ? 'Saving...' : 'Save API Key' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.api-key-container {
  display: flex;
  justify-content: center;
  align-items: center;
  min-height: 100vh;
  padding: 20px;
  background-color: var(--background-primary);
}

.api-key-card {
  background-color: var(--background-secondary);
  padding: 2rem;
  border-radius: 16px;
  box-shadow: 0 4px 6px var(--shadow-color);
  max-width: 400px;
  width: 100%;
}

h2 {
  color: var(--text-primary);
  margin-bottom: 1rem;
}

p {
  color: var(--text-secondary);
  margin-bottom: 1.5rem;
  font-size: 0.9rem;
}

.input-group {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.button-group {
  display: flex;
  gap: 1rem;
}

input {
  width: 100%;
}

button {
  flex: 1;
}

button.secondary {
  background-color: transparent;
  border: 1px solid var(--accent-primary);
  color: var(--accent-primary);
}

button.secondary:hover {
  background-color: var(--accent-primary);
  color: white;
}

button:disabled {
  opacity: 0.7;
  cursor: not-allowed;
}
</style> 