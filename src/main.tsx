import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import { maybeAutoStart } from './ipc/selftest';
import './styles/global.css';
import { applyUiSize, readUiSize } from './utils/appearance';
applyUiSize(readUiSize());

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);

// 开发期 GUI 自检：URL 加 ?selftest=1 触发
maybeAutoStart();
