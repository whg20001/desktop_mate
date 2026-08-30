if (!globalThis.ProgressEvent) {
  globalThis.ProgressEvent = class ProgressEvent extends Event {
    constructor(type, init = {}) {
      super(type);
      this.lengthComputable = Boolean(init.lengthComputable);
      this.loaded = Number(init.loaded ?? 0);
      this.total = Number(init.total ?? 0);
    }
  };
}

const { MMDLoader } = await import('@moeru/three-mmd');

const url = process.argv[2];
if (!url) {
  console.error('Usage: node scripts/inspect-pmx.mjs <http-url-to-model.pmx>');
  process.exitCode = 2;
} else {
  const inspectionComplete = new Error('PMX_INSPECTION_COMPLETE');
  const loader = new MMDLoader().register(() => ({
    name: 'InspectionPlugin',
    afterParse(pmx) {
      console.log(JSON.stringify({
        header: {
          modelName: pmx.header.modelName,
          englishModelName: pmx.header.englishModelName,
          vertices: pmx.vertices.length,
          textures: pmx.textures,
          materials: pmx.materials.map((material) => material.name),
          bones: pmx.bones.map((bone) => bone.name),
          morphs: pmx.morphs.map((morph) => morph.name),
          rigidBodies: pmx.rigidBodies.length,
          joints: pmx.joints.length,
        },
      }, null, 2));
      throw inspectionComplete;
    },
  }));

  try {
    await loader.loadAsync(url);
  } catch (error) {
    if (error !== inspectionComplete) throw error;
  }
}
