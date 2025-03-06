<script setup lang="ts">
import { ref } from 'vue'
import YouTubeInput from './YouTubeInput.vue'
import LanguageSelector from './LanguageSelector.vue'
import ProcessingProgress from './ProcessingProgress.vue'

interface Language {
  code: string
  name: string
}

interface LanguagePair {
  source: Language
  target: Language
}

interface ProcessingInput {
  url: string
  outputPath: string
}

const isProcessing = ref(false)
const currentStep = ref('')
const progress = ref(0)
const error = ref('')

const selectedLanguages = ref<LanguagePair | null>(null)

const handleUrlSubmit = async (input: ProcessingInput) => {
  if (!selectedLanguages.value) {
    error.value = 'Please select source and target languages first'
    return
  }

  isProcessing.value = true
  error.value = ''
  
  try {
    // TODO: Implement the actual processing logic here
    // This is just a mock implementation for now
    console.log('Processing video:', {
      url: input.url,
      outputPath: input.outputPath,
      languages: selectedLanguages.value
    })

    currentStep.value = 'Downloading video'
    progress.value = 0
    
    // Simulate processing steps
    const steps = [
      'Downloading video',
      'Extracting audio',
      'Transcribing audio',
      'Translating text',
      'Generating speech',
      'Creating final video'
    ]

    for (const step of steps) {
      currentStep.value = step
      await new Promise(resolve => setTimeout(resolve, 1000))
      progress.value += Math.floor(100 / steps.length)
    }

    progress.value = 100
    setTimeout(() => {
      isProcessing.value = false
      progress.value = 0
      currentStep.value = ''
    }, 1000)
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'An unknown error occurred'
    isProcessing.value = false
  }
}

const handleLanguagesSelected = (languages: LanguagePair) => {
  selectedLanguages.value = languages
}
</script>

<template>
  <div class="main-layout">
    <header class="backdrop-blur">
      <div class="header-content">
        <h1>YouTube Video Translator</h1>
        <p class="description">
          Translate your favorite YouTube videos into any language with AI-powered
          voice translation
        </p>
      </div>
    </header>

    <main>
      <div class="content-card backdrop-blur">
        <LanguageSelector @languages-selected="handleLanguagesSelected" />
        <div class="divider"></div>
        <YouTubeInput @url-submit="handleUrlSubmit" :disabled="isProcessing" />
      </div>
      
      <div v-if="isProcessing || error" class="content-card backdrop-blur">
        <ProcessingProgress
          :current-step="currentStep"
          :progress="progress"
          :error="error"
        />
      </div>
    </main>
  </div>
</template>

<style scoped>
.main-layout {
  max-width: 1000px;
  margin: 0 auto;
  padding: 0 2rem;
}

header {
  position: sticky;
  top: 0;
  z-index: 100;
  padding: 1rem 0;
  margin: 0 -2rem;
  background-color: var(--background-secondary);
}

.header-content {
  max-width: 1000px;
  margin: 0 auto;
  padding: 0 2rem;
  text-align: center;
}

h1 {
  font-size: 2.5rem;
  font-weight: 700;
  margin-bottom: 0.5rem;
  background: linear-gradient(135deg, var(--accent-primary), var(--accent-secondary));
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  background-clip: text;
  letter-spacing: -0.02em;
}

.description {
  font-size: 1.1rem;
  color: var(--text-secondary);
  max-width: 600px;
  margin: 0 auto;
  font-weight: 400;
}

main {
  padding: 2rem 0;
  display: flex;
  flex-direction: column;
  gap: 2rem;
}

.content-card {
  background-color: var(--background-secondary);
  border-radius: 20px;
  padding: 2rem;
  box-shadow: 0 4px 24px var(--shadow-color);
}

.divider {
  height: 1px;
  background-color: var(--border-color);
  margin: 2rem 0;
}

@media (max-width: 768px) {
  .main-layout {
    padding: 0 1rem;
  }

  header {
    margin: 0 -1rem;
  }

  .header-content {
    padding: 0 1rem;
  }

  h1 {
    font-size: 2rem;
  }

  .description {
    font-size: 1rem;
  }

  .content-card {
    padding: 1.5rem;
  }
}
</style> 