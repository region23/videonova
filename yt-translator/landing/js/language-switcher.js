document.addEventListener('DOMContentLoaded', function() {
  // Get language switcher buttons
  const langButtons = document.querySelectorAll('.lang-btn');
  const htmlElement = document.documentElement;
  
  // Function to update page content based on selected language
  function setLanguage(lang) {
    // Update HTML lang attribute
    htmlElement.setAttribute('lang', lang);
    htmlElement.setAttribute('data-lang', lang);
    
    // Update document title
    if (lang === 'en') {
      document.title = 'Videonova - Translate YouTube videos to any language';
      document.querySelector('meta[name="description"]').setAttribute(
        'content', 
        'Open-source AI application for translating YouTube videos to any language. Download, transcribe, translate, and voice over videos with one click.'
      );
    } else {
      document.title = 'Videonova - Переводите YouTube видео на любой язык';
      document.querySelector('meta[name="description"]').setAttribute(
        'content', 
        'Открытое ИИ-приложение для перевода YouTube видео на любой язык. Загружайте, транскрибируйте, переводите и озвучивайте видео одним кликом.'
      );
    }
    
    // Update all elements with data-ru and data-en attributes
    const elementsWithTranslation = document.querySelectorAll('[data-' + lang + ']');
    elementsWithTranslation.forEach(element => {
      element.textContent = element.getAttribute('data-' + lang);
    });
    
    // Update language button status
    langButtons.forEach(button => {
      if (button.getAttribute('data-lang') === lang) {
        button.classList.add('active');
      } else {
        button.classList.remove('active');
      }
    });
    
    // Save language preference to localStorage
    localStorage.setItem('preferred-language', lang);
  }
  
  // Add click event to language buttons
  langButtons.forEach(button => {
    button.addEventListener('click', function() {
      const lang = this.getAttribute('data-lang');
      setLanguage(lang);
    });
  });
  
  // Load saved language preference or default to Russian
  const savedLanguage = localStorage.getItem('preferred-language') || 'ru';
  setLanguage(savedLanguage);
}); 