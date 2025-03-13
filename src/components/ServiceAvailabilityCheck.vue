<template>
  <div class="service-check-container">
    <h3 class="check-title">Проверка доступности сервисов</h3>
    
    <div v-if="isChecking" class="checking-status">
      <div class="status-indicator">
        <div class="loading-spinner"></div>
        <p>Проверка доступности сервисов...</p>
      </div>
    </div>
    
    <div v-else-if="checkResult" class="check-results">
      <ul class="services-list">
        <li :class="checkResult.youtube_available ? 'available' : 'blocked'">
          <span class="service-name">YouTube:</span> 
          <span class="service-status">{{ checkResult.youtube_available ? 'Доступен' : 'Заблокирован' }}</span>
        </li>
        <li :class="checkResult.openai_available ? 'available' : 'blocked'">
          <span class="service-name">OpenAI:</span> 
          <span class="service-status">{{ checkResult.openai_available ? 'Доступен' : 'Заблокирован' }}</span>
        </li>
      </ul>
      
      <div :class="['message-container', checkResult.vpn_required ? 'warning-message' : 'success-message']">
        <p>{{ checkResult.message }}</p>
        <button class="check-button" @click="checkServices(true)">
          Проверить снова
        </button>
      </div>
    </div>
    
    <div v-else-if="error" class="error-container">
      <p>{{ error }}</p>
      <button class="check-button" @click="checkServices()">
        Попробовать снова
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import { checkServicesAvailability, setupServiceCheckListeners } from '../utils/serviceAvailability';

// Определяем типы для результатов проверки
interface ServiceAvailabilityResult {
  youtube_available: boolean;
  openai_available: boolean;
  vpn_required: boolean;
  youtube_blocked: boolean;
  openai_blocked: boolean;
  message: string;
  is_retry: boolean;
}

// Реактивные состояния
const isChecking = ref(false);
const checkResult = ref<ServiceAvailabilityResult | null>(null);
const error = ref<string | null>(null);

// Ссылка на функцию очистки слушателей событий
let cleanupListeners: (() => Promise<void>) | null = null;

// Функция для проверки доступности сервисов
const checkServices = async (isRetry: boolean = false) => {
  try {
    isChecking.value = true;
    error.value = null;
    
    // Получаем результаты проверки
    checkResult.value = await checkServicesAvailability(isRetry);
  } catch (err) {
    console.error('Ошибка при проверке доступности сервисов:', err);
    error.value = `Ошибка проверки: ${err}`;
    checkResult.value = null;
  } finally {
    isChecking.value = false;
  }
};

// Настройка слушателей событий и первичная проверка
onMounted(async () => {
  // Настраиваем слушателей событий для отслеживания прогресса
  cleanupListeners = await setupServiceCheckListeners({
    onCheckStarted: (isRetry) => {
      console.log(`Начата ${isRetry ? 'повторная' : 'первичная'} проверка сервисов`);
      isChecking.value = true;
    },
    onYouTubeResult: (available) => {
      console.log(`YouTube доступен: ${available}`);
    },
    onOpenAIResult: (available) => {
      console.log(`OpenAI доступен: ${available}`);
    },
    onCheckCompleted: (result) => {
      console.log('Проверка завершена:', result);
      isChecking.value = false;
    }
  });
  
  // Выполняем проверку при монтировании компонента
  await checkServices();
});

// Очистка слушателей при размонтировании компонента
onUnmounted(async () => {
  if (cleanupListeners) {
    await cleanupListeners();
  }
});
</script>

<style scoped>
.service-check-container {
  background-color: var(--background-secondary, #f5f5f5);
  border-radius: 12px;
  padding: 0.75rem;
  margin-bottom: 0.5rem;
  border: 1px solid rgba(0, 0, 0, 0.05);
  font-size: 0.85rem;
}

.check-title {
  font-size: 0.85rem;
  font-weight: 600;
  color: var(--text-primary);
  margin: 0 0 0.5rem;
  letter-spacing: -0.01em;
}

.checking-status {
  display: flex;
  justify-content: center;
  padding: 0.5rem 0;
}

.status-indicator {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.loading-spinner {
  width: 16px;
  height: 16px;
  border: 2px solid rgba(0, 119, 255, 0.3);
  border-top-color: var(--primary-color, #0077ff);
  border-radius: 50%;
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.services-list {
  list-style: none;
  padding: 0;
  margin: 0 0 0.75rem;
}

.services-list li {
  padding: 0.5rem;
  margin-bottom: 0.5rem;
  border-radius: 4px;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

li.available {
  background-color: rgba(76, 217, 100, 0.1);
  color: var(--success-color, #4cd964);
  border: 1px solid var(--success-color, #4cd964);
}

li.blocked {
  background-color: rgba(255, 59, 48, 0.1);
  color: var(--error-color, #ff3b30);
  border: 1px solid var(--error-color, #ff3b30);
}

.service-name {
  font-weight: 600;
}

.message-container {
  padding: 0.75rem;
  border-radius: 4px;
  margin-top: 0.5rem;
}

.warning-message {
  background-color: rgba(255, 165, 0, 0.1);
  border: 1px solid #ff8c00;
}

.success-message {
  background-color: rgba(76, 217, 100, 0.1);
  border: 1px solid var(--success-color, #4cd964);
}

.error-container {
  background-color: rgba(255, 59, 48, 0.1);
  border: 1px solid var(--error-color, #ff3b30);
  padding: 0.75rem;
  border-radius: 4px;
}

.check-button {
  background-color: var(--primary-color, #0077ff);
  color: white;
  border: none;
  padding: 0.5rem 0.75rem;
  border-radius: 4px;
  cursor: pointer;
  margin-top: 0.5rem;
  font-weight: 500;
  font-size: 0.8rem;
  transition: background-color 0.2s ease;
}

.check-button:hover {
  background-color: var(--primary-color-dark, #0066dd);
}
</style> 