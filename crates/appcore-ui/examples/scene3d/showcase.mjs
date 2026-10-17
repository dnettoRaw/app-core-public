import {createCity} from './city.mjs';
// Bounded deterministic geometry owned by this example; no external assets.
export const SCENES={
 city:{name:'Fractal city',description:'Recursive towers + skybridges · WASD flight · Q/E altitude · Shift boost · R home'},
 lorenz:{name:'Lorenz attractor',description:'Six nearby seeds · CPU RK4 · GPU point rendering'},
 galaxy:{name:'Spiral galaxy',description:'12,000 stars · procedural spiral arms · GPU rotation'},
 terrain:{name:'Procedural landscape',description:'8,192 triangles · height-field geometry · depth shading'},
 waves:{name:'Wave surface',description:'8,192 triangles · vertex-shader displacement · depth shading'}
};
export function createShowcase(kind){
 if(!Object.hasOwn(SCENES,kind)||kind==='lorenz')throw new RangeError('showcase scene');
 if(kind==='city')return createCity();
 if(kind==='galaxy'){
  const points=new Float32Array(12000*3);let seed=42;
  const random=()=>{seed=(Math.imul(seed,1664525)+1013904223)>>>0;return seed/4294967296;};
  for(let i=0;i<12000;i++){const r=Math.sqrt(random())*4.2,a=(i%4)*Math.PI/2+r*1.2+(random()-.5)*.55;points.set([Math.cos(a)*r,(random()-.5)*.35,Math.sin(a)*r],i*3);}
  return {points,triangles:false};
 }
 const points=new Float32Array(64*64*6*3);let offset=0;
 const height=(x,z)=>kind==='terrain'?Math.sin(x*1.3)*Math.cos(z*1.1)*.65+Math.sin(x*2.7+z)*.22:0;
 for(let x=0;x<64;x++)for(let z=0;z<64;z++){
  const a=-4+x/8,b=-4+z/8;
  for(const [u,v] of [[a,b],[a+.125,b],[a,b+.125],[a+.125,b],[a+.125,b+.125],[a,b+.125]]){
   points.set([u,height(u,v),v],offset);offset+=3;
  }
 }
 return {points,triangles:true};
}
