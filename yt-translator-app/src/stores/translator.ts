import { defineStore } from 'pinia'
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

// Define language options
export interface LanguageOption {
  code: string
  name: string
}

// Define the possible process steps
export enum ProcessStatus {
  IDLE = 'idle',
  DOWNLOADING = 'downloading',
  RECOGNIZING = 'recognizing',
  TRANSLATING = 'translating',
  GENERATING_SPEECH = 'generating_speech',
  MERGING = 'merging',
  COMPLETED = 'completed',
  ERROR = 'error'
}

// Define progress update interface
interface ProgressUpdate {
  status: ProcessStatus
  progress: number
  message?: string
  output_file?: string
}

export const useTranslatorStore = defineStore('translator', () => {
  // Input fields
  const youtubeUrl = ref('')
  const sourceLanguage = ref<string>('auto')
  const targetLanguage = ref<string>('en')
  const outputDirectory = ref<string>('')
  
  // Process state
  const processStatus = ref<ProcessStatus>(ProcessStatus.IDLE)
  const progress = ref<number>(0)
  const errorMessage = ref<string>('')
  const outputFilePath = ref<string>('')
  
  // Available languages
  const availableLanguages = ref<LanguageOption[]>([
    { code: 'auto', name: 'Auto-detect' },
    { code: 'en', name: 'English' },
    { code: 'ru', name: 'Russian' },
    { code: 'fr', name: 'French' },
    { code: 'de', name: 'German' },
    { code: 'es', name: 'Spanish' },
    { code: 'it', name: 'Italian' },
    { code: 'ja', name: 'Japanese' },
    { code: 'zh', name: 'Chinese' },
    { code: 'ko', name: 'Korean' },
    { code: 'ar', name: 'Arabic' },
    { code: 'hi', name: 'Hindi' },
    { code: 'pt', name: 'Portuguese' }
  ])
  
  // Reset state
  function resetState() {
    processStatus.value = ProcessStatus.IDLE
    progress.value = 0
    errorMessage.value = ''
    outputFilePath.value = ''
  }
  
  // Select output directory
  async function selectOutputDirectory() {
    try {
      outputDirectory.value = await invoke('select_directory')
    } catch (error) {
      console.error('Failed to select directory', error)
    }
  }
  
  // Start translation process
  async function startTranslation() {
    if (!youtubeUrl.value) {
      errorMessage.value = 'Please enter a YouTube URL'
      processStatus.value = ProcessStatus.ERROR
      return
    }
    
    if (!outputDirectory.value) {
      errorMessage.value = 'Please select an output directory'
      processStatus.value = ProcessStatus.ERROR
      return
    }
    
    resetState()
    processStatus.value = ProcessStatus.DOWNLOADING
    
    try {
      await invoke('start_translation_process', {
        request: {
          youtubeUrl: youtubeUrl.value,
          sourceLanguage: sourceLanguage.value,
          targetLanguage: targetLanguage.value,
          outputDirectory: outputDirectory.value
        }
      })
    } catch (error) {
      processStatus.value = ProcessStatus.ERROR
      errorMessage.value = `Error: ${error}`
    }
  }
  
  // Open output file
  async function openOutputFile() {
    if (outputFilePath.value) {
      try {
        await invoke('open_file', { path: outputFilePath.value })
      } catch (error) {
        console.error('Failed to open file', error)
      }
    }
  }
  
  // Listen for progress updates from the backend
  onMounted(async () => {
    const unlisten = await listen<ProgressUpdate>('translation_progress', (event) => {
      const update = event.payload
      
      processStatus.value = update.status
      progress.value = update.progress
      
      if (update.message) {
        errorMessage.value = update.message
      }
      
      if (update.output_file) {
        outputFilePath.value = update.output_file
      }
    })
    
    // Clean up listener when component is unmounted
    return () => {
      unlisten()
    }
  })
  
  return {
    // State
    youtubeUrl,
    sourceLanguage,
    targetLanguage,
    outputDirectory,
    processStatus,
    progress,
    errorMessage,
    outputFilePath,
    availableLanguages,
    
    // Actions
    resetState,
    selectOutputDirectory,
    startTranslation,
    openOutputFile
  }
}) 