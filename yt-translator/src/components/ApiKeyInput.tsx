import { useState } from 'react';
import { Store } from '@tauri-apps/plugin-store';
import { Button, Input, Card, message } from 'antd';

interface ApiKeyInputProps {
  onApiKeySet: () => void;
}

const store = new Store('.settings.dat');

export const ApiKeyInput = ({ onApiKeySet }: ApiKeyInputProps) => {
  const [apiKey, setApiKey] = useState('');
  const [loading, setLoading] = useState(false);

  const handleSubmit = async () => {
    if (!apiKey.trim()) {
      message.error('Please enter your OpenAI API key');
      return;
    }

    try {
      setLoading(true);
      // Store the API key securely
      await store.set('openai-api-key', apiKey);
      await store.save();
      message.success('API key saved successfully');
      onApiKeySet();
    } catch (error) {
      message.error('Failed to save API key');
      console.error('Error saving API key:', error);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div style={{ 
      display: 'flex', 
      justifyContent: 'center', 
      alignItems: 'center', 
      height: '100vh',
      background: '#f0f2f5'
    }}>
      <Card 
        title="OpenAI API Key Setup" 
        style={{ width: 400, boxShadow: '0 4px 8px rgba(0,0,0,0.1)' }}
      >
        <p>Please enter your OpenAI API key to continue. Your key will be stored securely.</p>
        <Input.Password
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder="Enter your OpenAI API key"
          style={{ marginBottom: 16 }}
        />
        <Button 
          type="primary" 
          onClick={handleSubmit} 
          loading={loading}
          block
        >
          Save API Key
        </Button>
      </Card>
    </div>
  );
}; 