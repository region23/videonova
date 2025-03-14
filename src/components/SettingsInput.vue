<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { message } from 'ant-design-vue'
import { invoke } from '@tauri-apps/api/core'
import { load } from '@tauri-apps/plugin-store'

interface Props {
  mode?: 'setup' | 'update'
  onCancel?: () => void
}

withDefaults(defineProps<Props>(), {
  mode: 'update'
})

// TTS Engine options
interface TtsEngineOption {
  value: string
  label: string
}

interface VoiceOption {
  value: string
  label: string
  description?: string
}

const engines = ref<TtsEngineOption[]>([
  { value: 'openai', label: 'OpenAI TTS (cloud)' },
  { value: 'fishspeech', label: 'Fish Speech (local)' }
])

// OpenAI voice options
const openaiVoices = ref<VoiceOption[]>([
  { value: 'alloy', label: 'Alloy', description: 'Universal voice with medium tone' },
  { value: 'echo', label: 'Echo', description: 'Medium tone voice' },
  { value: 'fable', label: 'Fable', description: 'Voice with feminine notes' },
  { value: 'onyx', label: 'Onyx', description: 'Rich and deep male voice' },
  { value: 'nova', label: 'Nova', description: 'Soft, clear female voice' },
  { value: 'shimmer', label: 'Shimmer', description: 'Bright female voice' }
])

// Fish Speech voices will be loaded from API
const fishSpeechVoices = ref<VoiceOption[]>([])

const emit = defineEmits(['settingsSaved', 'cancel'])
const apiKey = ref('')
const selectedEngine = ref<string>('openai')
const selectedOpenAIVoice = ref<string>('alloy')
const selectedFishSpeechVoice = ref<string>('')
const loading = ref(false)
const isFishSpeechInstalled = ref(false)

// Load settings when component mounts
onMounted(async () => {
  try {
    loading.value = true

    // Check if Fish Speech is installed
    isFishSpeechInstalled.value = await invoke('check_fish_speech_installed')

    // Get available engines
    const availableEngines = await invoke<string[]>('get_tts_engines')
    
    // If Fish Speech is not installed, remove it from the list
    if (!isFishSpeechInstalled.value) {
      engines.value = engines.value.filter(engine => engine.value !== 'fishspeech')
    } else {
      // If installed, load available voices
      try {
        const voices = await invoke('list_fish_speech_voices')
        if (Array.isArray(voices)) {
          fishSpeechVoices.value = voices.map(voice => ({
            value: voice.id,
            label: voice.name,
            description: voice.description || `Voice language: ${voice.language}`
          }))
        }
      } catch (err) {
        console.error('Failed to load Fish Speech voices:', err)
      }
    }

    // Get current settings
    const store = await load('.settings.dat')
    apiKey.value = await store.get('openai-api-key') as string || ''

    // Get TTS settings
    try {
      const ttsConfig = await invoke('get_tts_config')
      if (ttsConfig && typeof ttsConfig === 'object') {
        const config = ttsConfig as any
        
        if ('engine' in config) {
          selectedEngine.value = config.engine === 'OpenAI' ? 'openai' : 'fishspeech'
        }
        
        // Load voices
        if ('openai_voice' in config && config.openai_voice) {
          selectedOpenAIVoice.value = config.openai_voice
        }
        
        if ('fish_speech_voice_id' in config && config.fish_speech_voice_id) {
          selectedFishSpeechVoice.value = config.fish_speech_voice_id
        } else if (fishSpeechVoices.value.length > 0) {
          selectedFishSpeechVoice.value = fishSpeechVoices.value[0].value
        }
      } else {
        // If settings are missing, set default engine
        selectedEngine.value = await invoke('get_default_tts_engine')
      }
    } catch (error) {
      console.error('Failed to load TTS config:', error)
      // If error loading configuration, get default engine
      selectedEngine.value = await invoke('get_default_tts_engine')
    }
  } catch (error) {
    console.error('Error loading settings:', error)
    message.error('Failed to load settings')
  } finally {
    loading.value = false
  }
})

// Validate API key
const validateApiKey = async (key: string): Promise<boolean> => {
  if (!key || key.trim() === '') {
    return false
  }
  
  try {
    return await invoke('validate_openai_key', { apiKey: key })
  } catch (error) {
    console.error('Error validating API key:', error)
    return false
  }
}

// Install Fish Speech
const installFishSpeech = async () => {
  try {
    loading.value = true
    message.info('Starting Fish Speech installation. This may take several minutes...')
    
    await invoke('install_fish_speech')
    
    isFishSpeechInstalled.value = true
    message.success('Fish Speech installed successfully')
    
    // Add Fish Speech to the engine list if it's not there
    if (!engines.value.some(engine => engine.value === 'fishspeech')) {
      engines.value.push({ value: 'fishspeech', label: 'Fish Speech (local)' })
    }
    
    // Load Fish Speech voices
    try {
      const voices = await invoke('list_fish_speech_voices')
      if (Array.isArray(voices)) {
        fishSpeechVoices.value = voices.map(voice => ({
          value: voice.id,
          label: voice.name,
          description: voice.description || `Voice language: ${voice.language}`
        }))
        
        if (fishSpeechVoices.value.length > 0) {
          selectedFishSpeechVoice.value = fishSpeechVoices.value[0].value
        }
      }
    } catch (err) {
      console.error('Failed to load Fish Speech voices:', err)
    }
  } catch (error) {
    console.error('Error installing Fish Speech:', error)
    message.error('Error installing Fish Speech')
  } finally {
    loading.value = false
  }
}

// Save settings
const handleSubmit = async () => {
  try {
    loading.value = true
    
    // If OpenAI is selected, check API key
    if (selectedEngine.value === 'openai') {
      const isValid = await validateApiKey(apiKey.value)
      if (!isValid) {
        message.error('Invalid OpenAI API key')
        return
      }
    }
    
    // Save API key
    const store = await load('.settings.dat')
    await store.set('openai-api-key', apiKey.value)
    await store.save()
    
    // Save TTS settings
    await invoke('save_tts_config', {
      config: {
        engine: selectedEngine.value === 'openai' ? 'OpenAI' : 'FishSpeech',
        openai_voice: selectedOpenAIVoice.value,
        fish_speech_voice_id: selectedFishSpeechVoice.value || null,
        fish_speech_use_gpu: true
      }
    })
    
    message.success('Settings saved successfully')
    emit('settingsSaved')
  } catch (error) {
    console.error('Error saving settings:', error)
    message.error('Failed to save settings')
  } finally {
    loading.value = false
  }
}

// Cancel function
const handleCancel = () => {
  emit('cancel')
}
</script>

<template>
  <div class="settings-container backdrop-blur">
    <div class="settings-card">
      <h2>Settings</h2>
      
      <!-- API Key Section -->
      <div class="settings-section">
        <h3>OpenAI API Key</h3>
        <p>Your API key is used for text-to-speech, VTT creation, and translation</p>
        
        <div class="input-group">
          <label for="openai-api-key">OpenAI API Key:</label>
          <input
            id="openai-api-key"
            type="password"
            v-model="apiKey"
            placeholder="Enter your OpenAI API key"
            :disabled="loading"
          />
        </div>
      </div>
      
      <!-- TTS Settings Section -->
      <div class="settings-section">
        <h3>Text-to-Speech Settings</h3>
        <p>Choose your preferred TTS engine for video narration</p>
        
        <div class="input-group">
          <div class="engine-selector">
            <label for="tts-engine">TTS Engine:</label>
            <select 
              id="tts-engine" 
              v-model="selectedEngine" 
              :disabled="loading"
            >
              <option 
                v-for="engine in engines" 
                :key="engine.value" 
                :value="engine.value"
              >
                {{ engine.label }}
              </option>
            </select>
          </div>
          
          <!-- OpenAI Settings -->
          <div v-if="selectedEngine === 'openai'" class="voice-selector">
            <label for="openai-voice">OpenAI Voice:</label>
            <select 
              id="openai-voice" 
              v-model="selectedOpenAIVoice" 
              :disabled="loading"
            >
              <option 
                v-for="voice in openaiVoices" 
                :key="voice.value" 
                :value="voice.value"
              >
                {{ voice.label }} - {{ voice.description }}
              </option>
            </select>
          </div>
          
          <!-- Fish Speech Settings -->
          <div v-if="selectedEngine === 'fishspeech'">
            <!-- Show install button if Fish Speech is not installed -->
            <div v-if="!isFishSpeechInstalled" class="install-button-container">
              <button 
                @click="installFishSpeech"
                :disabled="loading"
                class="install-button"
              >
                {{ loading ? 'Installing...' : 'Install Fish Speech' }}
              </button>
              <p class="install-note">Note: Installation takes about 5-10 minutes and requires downloading ~2GB of data</p>
            </div>
            
            <!-- Voice selector if Fish Speech is installed -->
            <div v-else-if="fishSpeechVoices.length > 0" class="voice-selector">
              <label for="fishspeech-voice">Fish Speech Voice:</label>
              <select 
                id="fishspeech-voice" 
                v-model="selectedFishSpeechVoice" 
                :disabled="loading"
              >
                <option 
                  v-for="voice in fishSpeechVoices" 
                  :key="voice.value" 
                  :value="voice.value"
                >
                  {{ voice.label }} {{ voice.description ? `- ${voice.description}` : '' }}
                </option>
              </select>
            </div>
            
            <div v-else-if="isFishSpeechInstalled" class="no-voices-message">
              <p>No Fish Speech voices found. Check installation.</p>
            </div>
          </div>
        </div>
      </div>
      
      <div class="button-group">
        <button 
          v-if="mode === 'update'"
          @click="handleCancel"
          :disabled="loading"
          class="secondary"
        >
          Cancel
        </button>
        <button 
          @click="handleSubmit"
          :disabled="loading || (selectedEngine === 'openai' && !apiKey) || (selectedEngine === 'fishspeech' && !isFishSpeechInstalled)"
          class="primary"
        >
          {{ loading ? 'Saving...' : 'Save Settings' }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.settings-container {
  display: flex;
  justify-content: center;
  align-items: center;
  min-height: 100vh;
  padding: 20px;
  background-color: var(--background-primary);
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 1000;
}

.settings-card {
  background-color: var(--background-secondary);
  padding: 1.5rem;
  border-radius: 12px;
  box-shadow: 0 4px 6px var(--shadow-color);
  max-width: 500px;
  width: 100%;
}

h2 {
  color: var(--text-primary);
  margin-bottom: 0.75rem;
  font-size: 1rem;
}

h3 {
  color: var(--text-primary);
  margin-bottom: 0.25rem;
  font-size: 0.85rem;
}

p {
  color: var(--text-secondary);
  margin-bottom: 0.75rem;
  font-size: 0.7rem;
}

.settings-section {
  margin-bottom: 1rem;
  padding-bottom: 1rem;
  border-bottom: 1px solid var(--border-color);
}

.settings-section:last-of-type {
  border-bottom: none;
}

.input-group {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.engine-selector,
.voice-selector {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

label {
  font-size: 0.7rem;
  font-weight: 500;
  color: var(--text-secondary);
}

select, input {
  padding: 0.4rem 0.5rem;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background-color: var(--background-primary);
  color: var(--text-primary);
  font-size: 0.7rem;
}

select:focus, input:focus {
  outline: none;
  border-color: var(--accent-primary);
  box-shadow: 0 0 0 1px rgba(var(--accent-primary-rgb), 0.2);
}

.button-group {
  display: flex;
  gap: 0.75rem;
  margin-top: 1rem;
}

button {
  padding: 0.4rem 0.75rem;
  border-radius: 6px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
  flex: 1;
  font-size: 0.7rem;
}

button.primary {
  background-color: var(--accent-primary);
  color: white;
  border: none;
}

button.primary:hover {
  background-color: var(--accent-primary-dark);
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

.install-button-container {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.install-button {
  background-color: var(--success-color);
  color: white;
  border: none;
  border-radius: 6px;
  padding: 0.4rem 0.75rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
  font-size: 0.7rem;
}

.install-button:hover {
  background-color: var(--success-color-dark);
}

.install-note {
  font-size: 0.6rem;
  color: var(--text-secondary);
  margin: 0;
}

.no-voices-message {
  padding: 0.4rem;
  border-radius: 6px;
  background-color: var(--background-warning);
  color: var(--text-warning);
  font-size: 0.7rem;
}

.no-voices-message p {
  margin: 0;
  color: inherit;
}
</style> 