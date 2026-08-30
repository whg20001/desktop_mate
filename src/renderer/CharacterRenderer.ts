import * as THREE from 'three';

export class CharacterRenderer {
  readonly scene = new THREE.Scene();
  readonly camera: THREE.PerspectiveCamera;
  readonly renderer: THREE.WebGLRenderer;
  private readonly hemisphereLight: THREE.HemisphereLight;
  private readonly directionalLight: THREE.DirectionalLight;

  constructor(readonly canvas: HTMLCanvasElement) {
    this.scene.background = null;
    this.camera = new THREE.PerspectiveCamera(28, 1, 0.01, 1000);
    this.camera.position.set(0, 10, 45);

    this.renderer = new THREE.WebGLRenderer({
      canvas,
      alpha: true,
      antialias: true,
      premultipliedAlpha: true,
      powerPreference: 'high-performance',
    });
    this.renderer.setClearColor(0x000000, 0);
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));

    this.hemisphereLight = new THREE.HemisphereLight(0xffffff, 0x697386, 0.3);
    this.directionalLight = new THREE.DirectionalLight(0xfff6eb, 1.1);
    this.directionalLight.position.set(-5, 10, 8);
    this.scene.add(this.hemisphereLight, this.directionalLight);

    this.resize();
  }

  fit(bounds: THREE.Box3): void {
    const size = bounds.getSize(new THREE.Vector3());
    const center = bounds.getCenter(new THREE.Vector3());
    const verticalFov = THREE.MathUtils.degToRad(this.camera.fov);
    const fitHeightDistance = size.y / (2 * Math.tan(verticalFov / 2));
    const horizontalFov = 2 * Math.atan(Math.tan(verticalFov / 2) * this.camera.aspect);
    const fitWidthDistance = size.x / (2 * Math.tan(horizontalFov / 2));
    const distance = Math.max(fitHeightDistance, fitWidthDistance) * 1.14;

    this.camera.position.set(center.x, center.y + size.y * 0.01, center.z + distance);
    this.camera.near = Math.max(distance / 100, 0.01);
    this.camera.far = distance * 10;
    this.camera.lookAt(center.x, center.y + size.y * 0.01, center.z);
    this.camera.updateProjectionMatrix();
  }

  resize(): void {
    const width = Math.max(this.canvas.clientWidth, 1);
    const height = Math.max(this.canvas.clientHeight, 1);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.setSize(width, height, false);
  }

  setDisplayScale(scale: number): void {
    this.camera.zoom = scale;
    this.camera.updateProjectionMatrix();
  }

  setLighting(hemisphereIntensity: number, directionalIntensity: number): void {
    this.hemisphereLight.intensity = hemisphereIntensity;
    this.directionalLight.intensity = directionalIntensity;
  }

  render(): void {
    this.renderer.render(this.scene, this.camera);
  }

  dispose(): void {
    this.renderer.dispose();
  }
}

