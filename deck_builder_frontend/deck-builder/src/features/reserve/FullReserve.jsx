


import Reserve from "./Reserve"
export default function FullReserve() {
  return (
    <div id="deck-area">
      <Reserve
        title="Main Deck"
        count={0}
        price={12}
      />

      <Reserve
        title="Extra Deck"
        count={0}
        price={8}
      />

      <Reserve
        title="Side Deck"
        count={0}
        price={5}
      />
    </div>
  )
}