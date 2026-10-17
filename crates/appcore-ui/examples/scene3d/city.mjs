// Finite recursive architecture, not an infinite fractal or a physics engine.
export function createCity(){
 const vertices=[],normals=[];let boxes=0;
 function box(x,y,z,w,h,d){
  if(++boxes>512)throw new RangeError('city geometry budget');
  const corners=[[x-w/2,y,z-d/2],[x+w/2,y,z-d/2],[x+w/2,y+h,z-d/2],[x-w/2,y+h,z-d/2],[x-w/2,y,z+d/2],[x+w/2,y,z+d/2],[x+w/2,y+h,z+d/2],[x-w/2,y+h,z+d/2]];
  const faces=[[[0,3,2,1],[0,0,-1]],[[4,5,6,7],[0,0,1]],[[0,4,7,3],[-1,0,0]],[[1,2,6,5],[1,0,0]],[[3,7,6,2],[0,1,0]],[[0,1,5,4],[0,-1,0]]];
  for(const [ids,n] of faces)for(const i of [0,1,2,0,2,3]){vertices.push(...corners[ids[i]]);normals.push(...n);}
 }
 function tower(x,z,y,size,depth){
  const height=size*1.45;
  box(x,y,z,size,height,size);box(x,y+height,z,size*1.12,size*.09,size*1.12);
  if(depth)for(const dx of [-1,1])for(const dz of [-1,1])tower(x+dx*size*.28,z+dz*size*.28,y+height+size*.09,size*.4,depth-1);
 }
 box(0,-.3,0,26,.3,26);
 for(let x=-1;x<=1;x++)for(let z=-1;z<=1;z++)tower(x*7,z*7,0,3.2,2);
 // Elevated connectors leave streets open beneath them.
 for(let x=-1;x<=1;x++)for(let z=-1;z<1;z++)box(x*7,3,z*7+3.5,.65,.2,7);
 for(let z=-1;z<=1;z++)for(let x=-1;x<1;x++)box(x*7+3.5,3,z*7,7,.2,.65);
 return {points:new Float32Array(vertices),normals:new Float32Array(normals),triangles:true,boxes};
}
export function flightInitial(){return {position:[0,2,18],yaw:0,pitch:-.12};}
export function moveFlight(camera,keys,dt){
 if(!Number.isFinite(dt)||dt<0||dt>.05)throw new RangeError('flight timestep');
 const forward=Number(keys.has('KeyW'))-Number(keys.has('KeyS'));
 const side=Number(keys.has('KeyD'))-Number(keys.has('KeyA'));
 const vertical=Number(keys.has('KeyE'))-Number(keys.has('KeyQ'));
 const length=Math.hypot(forward,side,vertical)||1,speed=(keys.has('ShiftLeft')||keys.has('ShiftRight')?9:3)*dt/length;
 const s=Math.sin(camera.yaw),c=Math.cos(camera.yaw),p=Math.cos(camera.pitch);
 camera.position[0]+=(-s*p*forward+c*side)*speed;
 camera.position[1]+=(Math.sin(camera.pitch)*forward+vertical)*speed;
 camera.position[2]+=(-c*p*forward-s*side)*speed;
 for(let i=0;i<3;i++)camera.position[i]=Math.max(i===1?.3:-40,Math.min(40,camera.position[i]));
}
