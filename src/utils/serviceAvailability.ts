import { invoke } from '@tauri-apps/api/core';
import { message } from '@tauri-apps/plugin-dialog';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// Типы для результатов проверки
interface ServiceAvailabilityResult {
  youtube_available: boolean;
  openai_available: boolean;
  vpn_required: boolean;
  youtube_blocked: boolean;
  openai_blocked: boolean;
  message: string;
  is_retry: boolean;
}

// Настройки диалогового окна
interface DialogOptions {
  title?: string;
  kind?: 'info' | 'warning' | 'error';
}

/**
 * Проверяет доступность необходимых сервисов (YouTube и OpenAI)
 * и показывает диалоговое окно с рекомендациями, если сервисы недоступны
 * 
 * @param isRetry Указывает, что это повторная проверка после включения VPN
 * @returns Результат проверки доступности сервисов
 */
export async function checkServicesAvailability(isRetry: boolean = false): Promise<ServiceAvailabilityResult> {
  try {
    // Вызываем Rust-функцию для проверки доступности сервисов
    const result = await invoke<ServiceAvailabilityResult>('check_services_availability', { 
      is_retry: isRetry 
    });
    
    // Если VPN требуется, показываем диалоговое окно
    if (result.vpn_required) {
      const dialogOptions: DialogOptions = {
        title: isRetry ? 'VPN все еще требуется' : 'Требуется VPN',
        kind: 'warning'
      };
      
      try {
        // Показываем диалоговое окно и ждем, пока пользователь нажмет OK
        await message(result.message, dialogOptions);
        
        // Выполняем повторную проверку после закрытия диалога
        console.log('Пользователь закрыл диалог, выполняем повторную проверку...');
        return await checkServicesAvailability(true);
      } catch (e) {
        // Если произошла ошибка при показе диалога, просто возвращаем результат
        console.error('Ошибка при показе диалога:', e);
        return result;
      }
    } else if (isRetry) {
      // Если это повторная проверка и VPN теперь работает, показываем сообщение об успехе
      try {
        await message(result.message, { 
          title: 'VPN работает', 
          kind: 'info' 
        });
      } catch (e) {
        console.error('Ошибка при показе успешного диалога:', e);
      }
    }
    
    return result;
  } catch (error) {
    console.error('Ошибка при проверке доступности сервисов:', error);
    
    // В случае ошибки показываем диалоговое окно с сообщением об ошибке
    try {
      await message(
        `Произошла ошибка при проверке доступности сервисов: ${error}. 
        Пожалуйста, проверьте ваше интернет-соединение и попробуйте снова.`,
        { title: 'Ошибка проверки', kind: 'error' }
      );
    } catch (e) {
      console.error('Не удалось показать диалог с ошибкой:', e);
    }
    
    // Возвращаем объект с ошибкой
    return {
      youtube_available: false,
      openai_available: false,
      vpn_required: true,
      youtube_blocked: true,
      openai_blocked: true,
      message: `Ошибка проверки: ${error}`,
      is_retry: isRetry
    };
  }
}

/**
 * Настраивает слушателей событий для отслеживания процесса проверки
 * Можно использовать для обновления UI во время проверки
 * 
 * @param callbacks Объект с функциями обратного вызова для различных событий
 * @returns Функция для удаления слушателей событий
 */
export function setupServiceCheckListeners(callbacks: {
  onCheckStarted?: (isRetry: boolean) => void;
  onYouTubeChecking?: () => void;
  onYouTubeResult?: (available: boolean) => void;
  onOpenAIChecking?: () => void;
  onOpenAIResult?: (available: boolean) => void;
  onCheckCompleted?: (result: { vpn_required: boolean, is_retry: boolean }) => void;
}): Promise<() => Promise<void>> {
  // Массив функций для удаления слушателей
  const unlisteners: Promise<UnlistenFn>[] = [];
  
  // Слушаем событие начала проверки
  unlisteners.push(listen('services-check-started', (event) => {
    const data = event.payload as { is_retry: boolean };
    callbacks.onCheckStarted?.(data.is_retry);
  }));
  
  // Слушаем события проверки YouTube
  unlisteners.push(listen('checking-youtube', () => {
    callbacks.onYouTubeChecking?.();
  }));
  
  unlisteners.push(listen('youtube-check-complete', (event) => {
    callbacks.onYouTubeResult?.(event.payload as boolean);
  }));
  
  // Слушаем события проверки OpenAI
  unlisteners.push(listen('checking-openai', () => {
    callbacks.onOpenAIChecking?.();
  }));
  
  unlisteners.push(listen('openai-check-complete', (event) => {
    callbacks.onOpenAIResult?.(event.payload as boolean);
  }));
  
  // Слушаем событие завершения проверки
  unlisteners.push(listen('services-check-completed', (event) => {
    const data = event.payload as { vpn_required: boolean, is_retry: boolean };
    callbacks.onCheckCompleted?.(data);
  }));
  
  // Возвращаем функцию для удаления всех слушателей
  return Promise.all(unlisteners).then((unlisten) => {
    return async () => {
      for (const fn of unlisten) {
        await fn();
      }
    };
  });
}

/**
 * Пример использования в компоненте Vue 3:
 * 
 * <script setup lang="ts">
 * import { ref, onMounted, onUnmounted } from 'vue';
 * import { checkServicesAvailability, setupServiceCheckListeners } from '../utils/serviceAvailability';
 * 
 * const isChecking = ref(false);
 * const checkResult = ref(null);
 * 
 * let cleanupListeners: (() => Promise<void>) | null = null;
 * 
 * onMounted(async () => {
 *   // Настраиваем слушателей событий
 *   cleanupListeners = await setupServiceCheckListeners({
 *     onCheckStarted: (isRetry) => {
 *       console.log(`Начата ${isRetry ? 'повторная' : 'первичная'} проверка сервисов`);
 *       isChecking.value = true;
 *     },
 *     onCheckCompleted: (result) => {
 *       console.log('Проверка завершена:', result);
 *       isChecking.value = false;
 *     }
 *   });
 *   
 *   // Выполняем проверку при запуске компонента
 *   checkResult.value = await checkServicesAvailability();
 * });
 * 
 * onUnmounted(async () => {
 *   // Удаляем слушателей событий при уничтожении компонента
 *   if (cleanupListeners) {
 *     await cleanupListeners();
 *   }
 * });
 * 
 * const checkServicesAgain = async (isRetry = false) => {
 *   checkResult.value = await checkServicesAvailability(isRetry);
 * };
 * </script>
 */ 