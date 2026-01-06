import { useState } from 'react'
import FullReserve from './features/reserve/FullReserve'

function App() {
  const [count, setCount] = useState(0)

  return (
    <>
      <h1>Deck Builder</h1>
      <FullReserve />
    </>
  )
}

export default App
