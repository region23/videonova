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
  { value: 'openai', label: 'OpenAI TTS (облачный)' },
  { value: 'fishspeech', label: 'Fish Speech (локальный)' }
])

// Опции голосов OpenAI
const openaiVoices = ref<VoiceOption[]>([
  { value: 'alloy', label: 'Alloy', description: 'Универсальный голос со средним тоном' },
  { value: 'echo', label: 'Echo', description: 'Голос со средним тоном' },
  { value: 'fable', label: 'Fable', description: 'Голос с женственными нотками' },
  { value: 'onyx', label: 'Onyx', description: 'Насыщенный и глубокий мужской голос' },
  { value: 'nova', label: 'Nova', description: 'Мягкий, четкий женский голос' },
  { value: 'shimmer', label: 'Shimmer', description: 'Яркий женский голос' }
])

// Опции голосов Fish Speech будем получать из API
const fishSpeechVoices = ref<VoiceOption[]>([])

const emit = defineEmits(['settingsSaved', 'cancel'])
const apiKey = ref('')
const selectedEngine = ref<string>('openai')
const selectedOpenAIVoice = ref<string>('alloy')
const selectedFishSpeechVoice = ref<string>('')
const loading = ref(false)
const isFishSpeechInstalled = ref(false)

// Загрузка настроек при монтировании компонента
onMounted(async () => {
  try {
    loading.value = true

    // Проверим, установлен ли Fish Speech
    isFishSpeechInstalled.value = await invoke('check_fish_speech_installed')

    // Получаем доступные движки
    const availableEngines = await invoke<string[]>('get_tts_engines')
    
    // Если Fish Speech не установлен, удаляем его из списка
    if (!isFishSpeechInstalled.value) {
      engines.value = engines.value.filter(engine => engine.value !== 'fishspeech')
    } else {
      // Если установлен, загружаем доступные голоса
      try {
        const voices = await invoke('list_fish_speech_voices')
        if (Array.isArray(voices)) {
          fishSpeechVoices.value = voices.map(voice => ({
            value: voice.id,
            label: voice.name,
            description: voice.description || `Голос на языке: ${voice.language}`
          }))
        }
      } catch (err) {
        console.error('Failed to load Fish Speech voices:', err)
      }
    }

    // Получим текущие настройки
    const store = await load('.settings.dat')
    apiKey.value = await store.get('openai-api-key') as string || ''

    // Получим настройки TTS
    try {
      const ttsConfig = await invoke('get_tts_config')
      if (ttsConfig && typeof ttsConfig === 'object') {
        // Приведение типов и проверка наличия свойств
        const config = ttsConfig as any
        
        if ('engine' in config) {
          selectedEngine.value = config.engine === 'OpenAI' ? 'openai' : 'fishspeech'
        }
        
        // Загружаем голоса
        if ('openai_voice' in config && config.openai_voice) {
          selectedOpenAIVoice.value = config.openai_voice
        }
        
        if ('fish_speech_voice_id' in config && config.fish_speech_voice_id) {
          selectedFishSpeechVoice.value = config.fish_speech_voice_id
        } else if (fishSpeechVoices.value.length > 0) {
          selectedFishSpeechVoice.value = fishSpeechVoices.value[0].value
        }
      } else {
        // Если настройки отсутствуют, устанавливаем движок по умолчанию
        selectedEngine.value = await invoke('get_default_tts_engine')
      }
    } catch (error) {
      console.error('Failed to load TTS config:', error)
      // Если ошибка при загрузке конфигурации, получаем движок по умолчанию
      selectedEngine.value = await invoke('get_default_tts_engine')
    }
  } catch (error) {
    console.error('Error loading TTS settings:', error)
    message.error('Не удалось загрузить настройки TTS')
  } finally {
    loading.value = false
  }
})

// Валидация API ключа
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

// Установка Fish Speech
const installFishSpeech = async () => {
  try {
    loading.value = true
    message.info('Начинаем установку Fish Speech. Это может занять несколько минут...')
    
    await invoke('install_fish_speech')
    
    isFishSpeechInstalled.value = true
    message.success('Fish Speech успешно установлен')
    
    // Добавляем Fish Speech в список движков, если его там нет
    if (!engines.value.some(engine => engine.value === 'fishspeech')) {
      engines.value.push({ value: 'fishspeech', label: 'Fish Speech (локальный)' })
    }
    
    // Загружаем голоса Fish Speech
    try {
      const voices = await invoke('list_fish_speech_voices')
      if (Array.isArray(voices)) {
        fishSpeechVoices.value = voices.map(voice => ({
          value: voice.id,
          label: voice.name,
          description: voice.description || `Голос на языке: ${voice.language}`
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
    message.error('Ошибка при установке Fish Speech')
  } finally {
    loading.value = false
  }
}

// Сохранение настроек
const handleSubmit = async () => {
  try {
    loading.value = true
    
    // Если выбран OpenAI, проверяем API ключ
    if (selectedEngine.value === 'openai') {
      const isValid = await validateApiKey(apiKey.value)
      if (!isValid) {
        message.error('Некорректный OpenAI API ключ')
        return
      }
      
      // Сохраняем API ключ
      const store = await load('.settings.dat')
      await store.set('openai-api-key', apiKey.value)
      await store.save()
    }
    
    // Сохраняем настройки TTS
    await invoke('save_tts_config', {
      config: {
        engine: selectedEngine.value === 'openai' ? 'OpenAI' : 'FishSpeech',
        openai_voice: selectedOpenAIVoice.value,
        fish_speech_voice_id: selectedFishSpeechVoice.value || null,
        fish_speech_use_gpu: true
      }
    })
    
    message.success('Настройки сохранены')
    emit('settingsSaved')
  } catch (error) {
    console.error('Error saving TTS settings:', error)
    message.error('Не удалось сохранить настройки')
  } finally {
    loading.value = false
  }
}

// Функция отмены
const handleCancel = () => {
  emit('cancel')
}
</script>

<template>
  <div class="tts-settings-container backdrop-blur">
    <div class="tts-settings-card">
      <h2>Настройки синтеза речи (TTS)</h2>
      <p>Выберите предпочтительный способ синтеза речи для ваших видео</p>
      
      <div class="input-group">
        <div class="engine-selector">
          <label for="tts-engine">Движок TTS:</label>
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
        
        <!-- Настройки OpenAI -->
        <div v-if="selectedEngine === 'openai'" class="openai-settings">
          <!-- Ввод API ключа -->
          <div class="api-key-input">
            <label for="openai-api-key">OpenAI API ключ:</label>
            <input
              id="openai-api-key"
              type="password"
              v-model="apiKey"
              placeholder="Введите ваш OpenAI API ключ"
              :disabled="loading"
            />
          </div>
          
          <!-- Выбор голоса OpenAI -->
          <div class="voice-selector">
            <label for="openai-voice">Голос OpenAI:</label>
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
        </div>
        
        <!-- Настройки Fish Speech -->
        <div v-if="selectedEngine === 'fishspeech'" class="fishspeech-settings">
          <!-- Показываем кнопку установки, если Fish Speech не установлен -->
          <div v-if="!isFishSpeechInstalled" class="install-button-container">
            <button 
              @click="installFishSpeech"
              :disabled="loading"
              class="install-button"
            >
              {{ loading ? 'Установка...' : 'Установить Fish Speech' }}
            </button>
            <p class="install-note">Примечание: Установка занимает около 5-10 минут и требует загрузки ~2 ГБ данных</p>
          </div>
          
          <!-- Выбор голоса Fish Speech, если он установлен -->
          <div v-else-if="fishSpeechVoices.length > 0" class="voice-selector">
            <label for="fishspeech-voice">Голос Fish Speech:</label>
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
            <p>Голоса Fish Speech не найдены. Проверьте установку.</p>
          </div>
        </div>
        
        <div class="button-group">
          <button 
            v-if="mode === 'update'"
            @click="handleCancel"
            :disabled="loading"
            class="secondary"
          >
            Отмена
          </button>
          <button 
            @click="handleSubmit"
            :disabled="loading || (selectedEngine === 'openai' && !apiKey) || (selectedEngine === 'fishspeech' && !isFishSpeechInstalled)"
            class="primary"
          >
            {{ loading ? 'Сохранение...' : 'Сохранить настройки' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.tts-settings-container {
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

.tts-settings-card {
  background-color: var(--background-secondary);
  padding: 2rem;
  border-radius: 16px;
  box-shadow: 0 4px 6px var(--shadow-color);
  max-width: 500px;
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
  gap: 1.5rem;
}

.engine-selector,
.api-key-input,
.voice-selector {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.openai-settings,
.fishspeech-settings {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 1rem;
  background-color: rgba(var(--background-primary-rgb), 0.5);
}

label {
  font-size: 0.9rem;
  font-weight: 500;
  color: var(--text-secondary);
}

select, input {
  padding: 0.75rem;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background-color: var(--background-primary);
  color: var(--text-primary);
  font-size: 1rem;
}

select:focus, input:focus {
  outline: none;
  border-color: var(--accent-primary);
  box-shadow: 0 0 0 2px rgba(var(--accent-primary-rgb), 0.2);
}

.button-group {
  display: flex;
  gap: 1rem;
  margin-top: 1rem;
}

button {
  padding: 0.75rem 1rem;
  border-radius: 8px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
  flex: 1;
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
  gap: 0.5rem;
}

.install-button {
  background-color: var(--success-color);
  color: white;
  border: none;
  border-radius: 8px;
  padding: 0.75rem 1rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
}

.install-button:hover {
  background-color: var(--success-color-dark);
}

.install-note {
  font-size: 0.8rem;
  color: var(--text-secondary);
  margin: 0;
}

.no-voices-message {
  padding: 0.75rem;
  border-radius: 8px;
  background-color: var(--background-warning);
  color: var(--text-warning);
  font-size: 0.9rem;
}

.no-voices-message p {
  margin: 0;
  color: inherit;
}
</style> 