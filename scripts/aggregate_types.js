// This script reads the type definitions from all pallets and the runtime's own types.
// Aggregates types and writes them to a single file "types.json" in the root of this repo.

const fs = require('fs');
const path = require('path');

// TODO: no working. Implement this
const skipDirectories = []
const palletsPath = path.join(__dirname, `../pallets`)

// Types that are native to the runtime itself (i.e. come from lib.rs)
// These specifics are from https://polkadot.js.org/docs/api/start/types.extend#impact-on-extrinsics
const runtimeTypeOverrides = {
  // "AccountInfo": "AccountInfoWithTripleRefCount",
  "LookupSource": "AccountId",
}

let allTypes = {
  ...runtimeTypeOverrides,
  "IpfsCid": "Text"
};

const typeFiles = fs.readdirSync(palletsPath, { withFileTypes: true })
  .filter(dirent => dirent.isDirectory())
  .map(dirent => path.join(palletsPath, `/${dirent.name}/types.json`))
  .filter(path => fs.existsSync(path))

// Aggregate types from all pallets into `allTypes`.
for (let jsonPath of typeFiles) {
  let palletTypes = JSON.parse(fs.readFileSync(jsonPath, 'utf8'));
  allTypes = {...allTypes, ...palletTypes};
}

// Write aggregated types into a single file:
fs.writeFileSync(
  path.join(__dirname, "../types.json"),
  JSON.stringify(allTypes, null, 2),
  'utf8'
);
