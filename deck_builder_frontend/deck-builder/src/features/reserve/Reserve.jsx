


export default function Reserve({ title, count, price }) {
  return (
    <div className="area">
      <h3>
        {title} (<span>{count}</span>, {price}€)
      </h3>

      <div className="dropzone"></div>
    </div>
  )
}