import path from "path";
import { validateFunctions } from "@betty-blocks/cli/build/functions/validateFunctions";
import { getAllWasmFunctionsWithVersions } from "@betty-blocks/cli/build/functions/functionDefinitions";
import { publishWasmBlockStoreFunctions } from "@betty-blocks/cli/build/functions/publishWasmBlockStoreFunctions";
import os from "os";
import fs from "fs-extra";
import Jaws from "@betty-blocks/jaws";

const args = process.argv.slice(2);

const [BLOCKSTORE_CLI_SECRET, branch] = args;

if (!BLOCKSTORE_CLI_SECRET) {
  throw new Error("No BLOCKSTORE_CLI_SECRET provided");
}

if (!branch) {
  throw new Error("No branch provided");
}

const workingDir = process.cwd();
const baseFunctionsPath = path.join(workingDir, "functions");
const { valid } = await validateFunctions(true, baseFunctionsPath);

if (!valid) {
  process.exit(1);
}

const functionNames = getAllWasmFunctionsWithVersions(baseFunctionsPath);

const authBBCli = path.join(os.homedir(), ".bb-cli.json");
if (!fs.existsSync(authBBCli)) {
  throw new Error("No authentication file found at: " + authBBCli);
}

const jaws = Jaws.getInstance({
  issuer: "cli",
  services: {
    cli: {
      secret: BLOCKSTORE_CLI_SECRET,
    },
  },
});

const { jwt } = jaws.sign("cli", {
  application_id: "native",
});

fs.writeJSONSync(
  authBBCli,
  {
    applicationMap: {},
    auth: {
      "jwt.cli": jwt,
    },
  },
  { spaces: 2 }
);

let blockstoreApiUrl = "https://my.bettyblock.com/block-store-api/internal/cli";
if (branch === "edge" || branch === "acceptance") {
  blockstoreApiUrl = blockstoreApiUrl.replace(
    "my.bettyblock.com",
    `my.${branch}.bettyblock.com`
  );
}

const config = fs.readJSONSync(path.join(workingDir, "config.json"));
fs.writeJSONSync(
  path.join(workingDir, "config.json"),
  {
    ...config,
    blockstoreApiUrl,
  },
  { spaces: 2 }
);

await publishWasmBlockStoreFunctions(baseFunctionsPath, functionNames);
