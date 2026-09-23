// The Halden fixture: four candidate desk lamps, each built from named
// parts and exported as a binary glTF file, the way an agent would send
// them with --artifact. Writes the files to fixtures/halden/, and beside
// them the harness fixture, the decided one, and the screenshots' review,
// each naming the files by path: npm run fixture
import fs from "node:fs";
import path from "node:path";
import * as THREE from "three";
import { GLTFExporter } from "three/examples/jsm/exporters/GLTFExporter.js";

// GLTFExporter reads blobs with a FileReader, which Node does not have
globalThis.FileReader = class {
  readAsArrayBuffer(blob) { blob.arrayBuffer().then((b) => { this.result = b; this.onloadend?.(); this.onload?.({ target: this }); }); }
  readAsDataURL(blob) { blob.arrayBuffer().then((b) => { this.result = `data:${blob.type};base64,${Buffer.from(b).toString("base64")}`; this.onloadend?.(); this.onload?.({ target: this }); }); }
};

const mat = (name, color, o = {}) => new THREE.MeshStandardMaterial({ name, color, roughness: 0.5, metalness: 0, ...o });
const part = (name, geometry, material, at = [0, 0, 0], rot = [0, 0, 0]) => {
  const m = new THREE.Mesh(geometry, material);
  m.name = name;
  m.position.set(...at);
  m.rotation.set(...rot);
  return m;
};
const group = (name, children, at = [0, 0, 0], rot = [0, 0, 0]) => {
  const g = new THREE.Group();
  g.name = name;
  g.position.set(...at);
  g.rotation.set(...rot);
  children.forEach((c) => g.add(c));
  return g;
};

// metres; a desk lamp is about 0.45 m tall
function pivot() {
  const oak = mat("Oak", "#b98a5a", { roughness: 0.7 });
  const steel = mat("Powder coat", "#1f2a2e", { roughness: 0.45 });
  const brass = mat("Brass", "#c9a45c", { metalness: 1, roughness: 0.3 });
  const glow = mat("Bulb", "#fff4d6", { emissive: "#ffdca0", emissiveIntensity: 1.4 });
  const upper = group("Upper arm", [
    part("Upper tube", new THREE.CylinderGeometry(0.007, 0.007, 0.2, 20), steel, [0, 0.1, 0]),
    part("Elbow", new THREE.SphereGeometry(0.014, 24, 16), brass, [0, 0.2, 0]),
    group("Head", [
      part("Shade", new THREE.ConeGeometry(0.07, 0.1, 40, 1, true), steel, [0, -0.05, 0]),
      part("Bulb", new THREE.SphereGeometry(0.022, 24, 16), glow, [0, -0.075, 0]),
    ], [0, 0.2, 0], [0, 0, 0.47]),
  ], [0, 0.2, 0], [0, 0, -1.05]);
  return group("Lamp", [
    part("Base", new THREE.CylinderGeometry(0.085, 0.09, 0.024, 48), oak, [0, 0.012, 0]),
    part("Switch", new THREE.CylinderGeometry(0.008, 0.008, 0.008, 20), brass, [0.055, 0.028, 0.03]),
    group("Lower arm", [
      part("Lower tube", new THREE.CylinderGeometry(0.007, 0.007, 0.2, 20), steel, [0, 0.1, 0]),
      part("Shoulder", new THREE.SphereGeometry(0.014, 24, 16), brass, [0, 0, 0]),
      upper,
    ], [-0.02, 0.028, 0], [0, 0, 0.28]),
  ]);
}

function arc() {
  const stone = mat("Terrazzo", "#d9d2c6", { roughness: 0.85 });
  const tube = mat("Anodised aluminium", "#a4552f", { metalness: 0.6, roughness: 0.35 });
  const inner = mat("Shade inside", "#f5efe4", { side: THREE.DoubleSide });
  const glow = mat("Bulb", "#fff4d6", { emissive: "#ffdca0", emissiveIntensity: 1.4 });
  const curve = new THREE.CubicBezierCurve3(new THREE.Vector3(0, 0.03, 0), new THREE.Vector3(0, 0.36, 0), new THREE.Vector3(0.12, 0.46, 0), new THREE.Vector3(0.26, 0.4, 0));
  const dome = new THREE.SphereGeometry(0.075, 40, 20, 0, Math.PI * 2, 0, Math.PI / 2);
  return group("Lamp", [
    part("Base", new THREE.BoxGeometry(0.16, 0.03, 0.12), stone, [0, 0.015, 0]),
    part("Arc", new THREE.TubeGeometry(curve, 56, 0.008, 12), tube),
    group("Head", [
      part("Dome", dome, tube),
      part("Dome inside", new THREE.SphereGeometry(0.073, 40, 20, 0, Math.PI * 2, 0, Math.PI / 2), inner),
      part("Bulb", new THREE.SphereGeometry(0.024, 24, 16), glow, [0, 0.02, 0]),
    ], [0.26, 0.39, 0], [Math.PI, 0, 0.35]),
    part("Cable", new THREE.TorusGeometry(0.03, 0.003, 8, 32, Math.PI), mat("Cable", "#222"), [-0.08, 0.01, 0], [Math.PI / 2, 0, Math.PI / 2]),
  ]);
}

function column() {
  const walnut = mat("Walnut", "#5b3a26", { roughness: 0.6 });
  const linen = mat("Linen", "#efe6d2", { roughness: 0.95, side: THREE.DoubleSide, emissive: "#ffcf8a", emissiveIntensity: 0.25 });
  const brass = mat("Brass", "#c9a45c", { metalness: 1, roughness: 0.3 });
  return group("Lamp", [
    part("Base", new THREE.CylinderGeometry(0.06, 0.065, 0.05, 48), walnut, [0, 0.025, 0]),
    part("Stem", new THREE.CylinderGeometry(0.012, 0.012, 0.26, 24), brass, [0, 0.18, 0]),
    part("Finial", new THREE.SphereGeometry(0.012, 20, 12), brass, [0, 0.44, 0]),
    part("Drum shade", new THREE.CylinderGeometry(0.1, 0.11, 0.14, 64, 1, true), linen, [0, 0.36, 0]),
  ]);
}

function tripod() {
  const ash = mat("Ash", "#d8bf97", { roughness: 0.7 });
  const paper = mat("Rice paper", "#fbf6ea", { roughness: 1, emissive: "#ffd89a", emissiveIntensity: 0.5 });
  const legs = [0, 1, 2].map((i) => {
    const a = (i / 3) * Math.PI * 2;
    return part(`Leg ${i + 1}`, new THREE.CylinderGeometry(0.006, 0.008, 0.3, 16), ash, [Math.cos(a) * 0.04, 0.145, Math.sin(a) * 0.04], [-Math.sin(a) * 0.25, 0, Math.cos(a) * 0.25]);
  });
  return group("Lamp", [
    group("Legs", legs),
    part("Collar", new THREE.TorusGeometry(0.018, 0.005, 12, 40), ash, [0, 0.29, 0], [Math.PI / 2, 0, 0]),
    part("Globe", new THREE.SphereGeometry(0.09, 40, 24), paper, [0, 0.38, 0]),
  ]);
}

async function glb(object) {
  // nothing is textured, so the UVs are dead weight
  object.traverse((o) => { if (o.isMesh) o.geometry.deleteAttribute("uv"); });
  const scene = new THREE.Scene();
  scene.add(object);
  return Buffer.from(await new GLTFExporter().parseAsync(scene, { binary: true }));
}

const models = [
  { id: "L1", name: "Pivot", build: pivot, reasoning: "The classic two-arm task lamp, in an oak base and dark powder coat. Brass only where it moves: the **joints and the switch**. Reads as a tool first." },
  { id: "L2", name: "Arc", build: arc, reasoning: "One bent aluminium tube from a terrazzo block to a dome. No joints, so it cannot be aimed, but it is the calmest silhouette of the four." },
  { id: "L3", name: "Column", build: column, reasoning: "A bedside lamp rather than a task lamp: walnut, a brass stem, a linen drum that glows. Included to check whether *task* is the right brief at all." },
  { id: "L4", name: "Tripod", build: tripod, reasoning: "Three ash legs and a paper globe. The softest light of the four, and the only one that looks good switched off in a photo." },
];

const payload = {
  notes: "Round 1 of the **Halden** desk lamp: four directions. Orbit each one, look at it from the desk (the *Seated* view), and click a part to ask for a change to it.",
  subject: { name: "Halden desk lamp", units: "m" },
  models: [],
};

const decision = {
  decisions: [
    { id: "L1", action: "favorite", note: "The direction. Warmer overall, it reads a little cold next to Column.", comments: [
      { target: "Lamp > Lower arm > Upper arm > Head > Shade", name: "Shade", material: "Powder coat", point: [0.1712, 0.3391, 0.0189], view: { position: [0.5504, 0.3213, 0.5521], target: [0, 0.2, 0] }, note: "Wider and shallower, so the bulb is hidden from the chair" },
      { target: "Lamp > Base", name: "Base", material: "Oak", point: [0.0341, 0.024, 0.0146], view: { position: [0.5542, 0.5538, 0.5682], target: [0.0204, 0.1922, 0] }, note: "A darker oak, closer to the walnut of Column" },
    ] },
    { id: "L3", action: "keep", note: "Keep it as the bedside one in a second line." },
    { id: "L2", action: "drop", note: "A lamp that cannot be aimed is not a desk lamp." },
  ],
  undecided: ["L4"],
};

const here = import.meta.dirname;
const files = path.resolve(here, "../fixtures/halden");
fs.mkdirSync(files, { recursive: true });
const artifacts = {};
for (const m of models) {
  const file = `${m.name.toLowerCase()}.glb`;
  fs.writeFileSync(path.join(files, file), await glb(m.build()));
  artifacts[file] = path.join(files, file);
  payload.models.push({ id: m.id, name: m.name, reasoning: m.reasoning, file: { $artifact: file }, views: [{ name: "Seated", position: [0.55, 0.32, 0.55], target: [0, 0.2, 0] }] });
}
// each fixture names the files by path, relative to itself
const relative = (fixture) =>
  Object.fromEntries(Object.entries(artifacts).map(([name, file]) => [name, { path: path.relative(path.dirname(path.resolve(here, fixture)), file) }]));
const write = (file, value) => fs.writeFileSync(path.resolve(here, file), JSON.stringify(value, null, 2) + "\n");
const title = "Halden desk lamp — round 1";
const origin = { repo: "halden/lamp", workflow: "design" };
const fixtures = {
  "../fixtures/halden.json": { title, origin, payload },
  "../fixtures/halden.decided.json": { title, origin, payload, decision: { decided_by: "maya", decided_at: "2026-09-23T17:40:00Z", data: decision }, agent_note: "Go with Pivot; the base and shade notes first." },
  "../../../e2e/screenshots/fixtures/19-model-halden.json": { plugin: "model", title, origin, requested_by: "claude", age: "40m", payload },
};
for (const [file, value] of Object.entries(fixtures)) write(file, { ...value, artifacts: relative(file) });
const sizes = Object.values(artifacts).map((f) => fs.statSync(f).size);
console.log(`${models.length} models, ${(sizes.reduce((a, b) => a + b, 0) / 1024).toFixed(0)} KB of GLB, payload ${(JSON.stringify(payload).length / 1024).toFixed(1)} KB`);
