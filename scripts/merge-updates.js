import fs from "fs"
import path from "path"

// Une los update-fragment.json de cada plataforma en un solo update.json
// Uso en CI: node scripts/merge-updates.js
// Espera artifacts/ con subdirs: linux-artifacts, macos-artifacts, windows-artifacts

const artifactsDir = "artifacts"
const outputPath = path.join(artifactsDir, "update.json")

const sources = ["linux-artifacts", "macos-artifacts", "windows-artifacts"]

let version, pub_date, notes
const platforms = {}

for (const name of sources) {
  const fragmentPath = path.join(artifactsDir, name, "update-fragment.json")
  if (!fs.existsSync(fragmentPath)) {
    console.warn(`⚠️  Fragmento no encontrado: ${fragmentPath}`)
    continue
  }
  const fragment = JSON.parse(fs.readFileSync(fragmentPath, "utf-8"))
  version = fragment.version
  pub_date = fragment.pub_date
  notes = fragment.notes
  Object.assign(platforms, fragment.platforms)
  console.log(`✅ ${name}: ${Object.keys(fragment.platforms).join(", ")}`)
}

if (!version || !Object.keys(platforms).length) {
  console.error("❌ No se encontraron fragmentos válidos")
  process.exit(1)
}

const update = { version, notes, pub_date, platforms }
fs.writeFileSync(outputPath, JSON.stringify(update, null, 2))
console.log(`\n✅ update.json generado (${Object.keys(platforms).join(", ")})`)
console.log(`   📄 ${outputPath}`)