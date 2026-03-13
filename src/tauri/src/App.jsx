import { useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import './App.css'

function App() {
  const [output, setOutput] = useState('')
  const [name, setName] = useState('')

  async function runHelloWorld() {
    const result = await invoke('hello_world')
    setOutput(result)
  }

  async function runGreet() {
    const result = await invoke('greet', { name: name || 'stranger' })
    setOutput(result)
  }

  return (
    <div className="app">
      <h2>bebop</h2>

      <div className="controls">
        <button onClick={runHelloWorld}>hello world</button>

        <div className="greet-row">
          <input
            value={name}
            onChange={e => setName(e.target.value)}
            placeholder="enter name"
          />
          <button onClick={runGreet}>greet</button>
        </div>
      </div>

      {output && <div className="output">{output}</div>}
    </div>
  )
}

export default App
