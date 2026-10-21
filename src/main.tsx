import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import MiniApp from './MiniApp'

// The mini window loads the same bundle with ?view=mini.
const mini = new URLSearchParams(location.search).get('view') === 'mini'

createRoot(document.getElementById('root')!).render(
  <StrictMode>{mini ? <MiniApp /> : <App />}</StrictMode>,
)
