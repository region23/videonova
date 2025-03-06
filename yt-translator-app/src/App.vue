<script setup lang="ts">
import { useTranslatorStore, ProcessStatus } from './stores/translator';
import { computed } from 'vue';

const store = useTranslatorStore();

// Computed properties for UI
const isProcessing = computed(() => 
  store.processStatus !== ProcessStatus.IDLE && 
  store.processStatus !== ProcessStatus.COMPLETED && 
  store.processStatus !== ProcessStatus.ERROR
);

const statusText = computed(() => {
  switch (store.processStatus) {
    case ProcessStatus.DOWNLOADING:
      return 'Downloading video...';
    case ProcessStatus.RECOGNIZING:
      return 'Recognizing speech...';
    case ProcessStatus.TRANSLATING:
      return 'Translating subtitles...';
    case ProcessStatus.GENERATING_SPEECH:
      return 'Generating speech from translation...';
    case ProcessStatus.MERGING:
      return 'Merging audio and video...';
    case ProcessStatus.COMPLETED:
      return 'Translation completed!';
    case ProcessStatus.ERROR:
      return `Error: ${store.errorMessage}`;
    default:
      return 'Ready to translate';
  }
});

// Function to handle open output file
const openOutputFile = async () => {
  if (store.outputFilePath) {
    // Will be implemented in Rust backend
    // await invoke('open_file', { path: store.outputFilePath });
  }
};
</script>

<template>
  <v-app>
    <v-app-bar color="primary" density="compact">
      <v-app-bar-title>YouTube Video Translator</v-app-bar-title>
    </v-app-bar>

    <v-main>
      <v-container>
        <v-card class="mx-auto my-4" max-width="800">
          <v-card-title class="text-h5">
            YouTube Video Translator
          </v-card-title>
          
          <v-card-text>
            <v-form @submit.prevent="store.startTranslation">
              <!-- YouTube URL input -->
              <v-text-field
                v-model="store.youtubeUrl"
                label="YouTube Video URL"
                placeholder="https://www.youtube.com/watch?v=..."
                variant="outlined"
                :disabled="isProcessing"
                prepend-inner-icon="mdi-youtube"
                required
              ></v-text-field>
              
              <!-- Language selection -->
              <div class="d-flex gap-4">
                <v-select
                  v-model="store.sourceLanguage"
                  :items="store.availableLanguages"
                  item-title="name"
                  item-value="code"
                  label="Source Language"
                  variant="outlined"
                  :disabled="isProcessing"
                  class="flex-grow-1"
                ></v-select>
                
                <v-select
                  v-model="store.targetLanguage"
                  :items="store.availableLanguages.filter(lang => lang.code !== 'auto')"
                  item-title="name"
                  item-value="code"
                  label="Target Language"
                  variant="outlined"
                  :disabled="isProcessing"
                  class="flex-grow-1"
                ></v-select>
              </div>
              
              <!-- Output directory selection -->
              <div class="d-flex align-center gap-2">
                <v-text-field
                  v-model="store.outputDirectory"
                  label="Output Directory"
                  variant="outlined"
                  readonly
                  :disabled="isProcessing"
                  class="flex-grow-1"
                ></v-text-field>
                
                <v-btn
                  color="secondary"
                  variant="outlined"
                  :disabled="isProcessing"
                  @click="store.selectOutputDirectory"
                >
                  Browse
                </v-btn>
              </div>
              
              <!-- Process controls -->
              <div class="text-center mt-4">
                <v-btn
                  color="primary"
                  size="large"
                  :loading="isProcessing"
                  :disabled="isProcessing"
                  type="submit"
                >
                  Translate Video
                </v-btn>
              </div>
            </v-form>
            
            <!-- Processing status -->
            <v-card v-if="store.processStatus !== ProcessStatus.IDLE" class="mt-6" :color="store.processStatus === ProcessStatus.ERROR ? 'error' : (store.processStatus === ProcessStatus.COMPLETED ? 'success' : 'info')">
              <v-card-text>
                <div class="text-center">
                  <p class="text-h6 mb-2">{{ statusText }}</p>
                  
                  <v-progress-linear
                    v-if="isProcessing"
                    :model-value="store.progress"
                    height="10"
                    color="white"
                    striped
                  ></v-progress-linear>
                  
                  <div v-if="store.processStatus === ProcessStatus.COMPLETED" class="mt-2">
                    <v-btn
                      color="white"
                      variant="outlined"
                      @click="openOutputFile"
                      class="me-2"
                    >
                      Open File
                    </v-btn>
                    
                    <v-btn
                      color="white"
                      variant="outlined"
                      @click="store.resetState"
                    >
                      Translate Another Video
                    </v-btn>
                  </div>
                  
                  <div v-if="store.processStatus === ProcessStatus.ERROR" class="mt-2">
                    <v-btn
                      color="white"
                      variant="outlined"
                      @click="store.resetState"
                    >
                      Try Again
                    </v-btn>
                  </div>
                </div>
              </v-card-text>
            </v-card>
          </v-card-text>
        </v-card>
      </v-container>
    </v-main>
    
    <v-footer app class="bg-secondary">
      <div class="text-center w-100">
        YouTube Video Translator v1.0.0
      </div>
    </v-footer>
  </v-app>
</template>

<style scoped>
.logo.vite:hover {
  filter: drop-shadow(0 0 2em #747bff);
}

.logo.vue:hover {
  filter: drop-shadow(0 0 2em #249b73);
}

</style>
<style>
:root {
  font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
  font-size: 16px;
  line-height: 24px;
  font-weight: 400;

  color: #0f0f0f;
  background-color: #f6f6f6;

  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  -webkit-text-size-adjust: 100%;
}

.container {
  margin: 0;
  padding-top: 10vh;
  display: flex;
  flex-direction: column;
  justify-content: center;
  text-align: center;
}

.logo {
  height: 6em;
  padding: 1.5em;
  will-change: filter;
  transition: 0.75s;
}

.logo.tauri:hover {
  filter: drop-shadow(0 0 2em #24c8db);
}

.row {
  display: flex;
  justify-content: center;
}

a {
  font-weight: 500;
  color: #646cff;
  text-decoration: inherit;
}

a:hover {
  color: #535bf2;
}

h1 {
  text-align: center;
}

input,
button {
  border-radius: 8px;
  border: 1px solid transparent;
  padding: 0.6em 1.2em;
  font-size: 1em;
  font-weight: 500;
  font-family: inherit;
  color: #0f0f0f;
  background-color: #ffffff;
  transition: border-color 0.25s;
  box-shadow: 0 2px 2px rgba(0, 0, 0, 0.2);
}

button {
  cursor: pointer;
}

button:hover {
  border-color: #396cd8;
}
button:active {
  border-color: #396cd8;
  background-color: #e8e8e8;
}

input,
button {
  outline: none;
}

#greet-input {
  margin-right: 5px;
}

@media (prefers-color-scheme: dark) {
  :root {
    color: #f6f6f6;
    background-color: #2f2f2f;
  }

  a:hover {
    color: #24c8db;
  }

  input,
  button {
    color: #ffffff;
    background-color: #0f0f0f98;
  }
  button:active {
    background-color: #0f0f0f69;
  }
}

</style>